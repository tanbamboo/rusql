//! Normalize MySQL GRANT/REVOKE object syntax and parse role statements.

use sqlparser::ast::{Ident, ObjectName, ObjectType, Statement};

/// Rewrite `ON db.*` to `ON ALL TABLES IN SCHEMA db` for sqlparser.
pub fn rewrite_grant_objects(sql: &str) -> String {
    let upper = sql.to_ascii_uppercase();
    let on_idx = match upper.find(" ON ") {
        Some(i) => i,
        None => return sql.to_string(),
    };
    let after_on = &sql[on_idx + 4..];
    let (object, rest_start) = match parse_grant_object(after_on) {
        Some(v) => v,
        None => return sql.to_string(),
    };
    let prefix = &sql[..on_idx + 4];
    let rest = &after_on[rest_start..];
    if object.ends_with(".*") {
        let schema = object.trim_end_matches(".*");
        format!("{prefix}ALL TABLES IN SCHEMA {schema}{rest}")
    } else {
        sql.to_string()
    }
}

fn parse_grant_object(input: &str) -> Option<(String, usize)> {
    let trimmed = input.trim_start();
    let offset = input.len() - trimmed.len();
    if let Some(stripped) = trimmed.strip_prefix('`') {
        let end = stripped.find('`')? + 1;
        let mut object = stripped[..end - 1].to_string();
        let mut pos = end + 1;
        if trimmed.get(pos..)?.starts_with(".*") {
            object.push_str(".*");
            pos += 2;
        }
        return Some((object, offset + pos));
    }
    let end = trimmed
        .find(|c: char| c.is_whitespace())
        .unwrap_or(trimmed.len());
    let object = trimmed[..end].to_string();
    Some((object, offset + end))
}

/// Role name with MySQL's default host `%` when the host is omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleName {
    pub user: String,
    pub host: String,
}

/// `SET ROLE` forms accepted in this slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetRole {
    None,
    All,
    /// No default-role catalog in this slice, so this activates nothing.
    Default,
    AllExcept(Vec<RoleName>),
    Named(Vec<RoleName>),
}

/// Role DDL that sqlparser does not model as MySQL `GRANT role TO user`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleStatement {
    Create {
        names: Vec<RoleName>,
        if_not_exists: bool,
    },
    Drop {
        names: Vec<RoleName>,
        if_exists: bool,
    },
    Grant {
        roles: Vec<RoleName>,
        grantees: Vec<RoleName>,
    },
    Revoke {
        roles: Vec<RoleName>,
        grantees: Vec<RoleName>,
    },
    Set(SetRole),
}

/// Parse outcome for a single COM_QUERY string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleParse {
    NotRole,
    Statement(RoleStatement),
}

pub fn role_statement_writes(stmt: &RoleStatement) -> bool {
    !matches!(stmt, RoleStatement::Set(_))
}

/// Parse one role statement. Mixed batches and privilege `GRANT ... ON` return [`RoleParse::NotRole`].
pub fn parse_role_statement(sql: &str) -> RoleParse {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return RoleParse::NotRole;
    }
    if starts_keyword(trimmed, "CREATE ROLE") {
        return parse_create_or_drop(trimmed, true);
    }
    if starts_keyword(trimmed, "DROP ROLE") {
        return parse_create_or_drop(trimmed, false);
    }
    if starts_keyword(trimmed, "SET ROLE") {
        return parse_set_role(trimmed);
    }
    if starts_keyword(trimmed, "GRANT") && !has_top_level_on(trimmed) {
        return parse_grant_or_revoke(trimmed, true);
    }
    if starts_keyword(trimmed, "REVOKE") && !has_top_level_on(trimmed) {
        return parse_grant_or_revoke(trimmed, false);
    }
    RoleParse::NotRole
}

/// Map sqlparser role AST that a multi-statement batch can produce.
pub fn role_statement_from_ast(stmt: &Statement) -> Option<RoleStatement> {
    match stmt {
        Statement::CreateRole {
            names,
            if_not_exists,
            ..
        } => Some(RoleStatement::Create {
            names: names.iter().map(role_name_from_object).collect(),
            if_not_exists: *if_not_exists,
        }),
        Statement::Drop {
            object_type,
            if_exists,
            names,
            ..
        } if *object_type == ObjectType::Role => Some(RoleStatement::Drop {
            names: names.iter().map(role_name_from_object).collect(),
            if_exists: *if_exists,
        }),
        Statement::SetRole { role_name, .. } => Some(RoleStatement::Set(match role_name {
            None => SetRole::None,
            Some(ident) => SetRole::Named(vec![role_name_from_ident(ident)]),
        })),
        _ => None,
    }
}

fn parse_create_or_drop(sql: &str, create: bool) -> RoleParse {
    let keyword_len = if create {
        "CREATE ROLE".len()
    } else {
        "DROP ROLE".len()
    };
    let mut rest = sql[keyword_len..].trim_start();
    let upper = rest.to_ascii_uppercase();
    let if_clause = if create {
        if upper.starts_with("IF NOT EXISTS") {
            rest = rest["IF NOT EXISTS".len()..].trim_start();
            true
        } else {
            false
        }
    } else if upper.starts_with("IF EXISTS") {
        rest = rest["IF EXISTS".len()..].trim_start();
        true
    } else {
        false
    };
    let Some((names, tail)) = parse_name_list(rest) else {
        return RoleParse::NotRole;
    };
    if !tail_empty(tail) || names.is_empty() {
        return RoleParse::NotRole;
    }
    if create {
        RoleParse::Statement(RoleStatement::Create {
            names,
            if_not_exists: if_clause,
        })
    } else {
        RoleParse::Statement(RoleStatement::Drop {
            names,
            if_exists: if_clause,
        })
    }
}

fn parse_set_role(sql: &str) -> RoleParse {
    let rest = sql["SET ROLE".len()..].trim_start();
    if rest.is_empty() {
        return RoleParse::NotRole;
    }
    if starts_keyword(rest, "NONE") && tail_empty(&rest["NONE".len()..]) {
        return RoleParse::Statement(RoleStatement::Set(SetRole::None));
    }
    if starts_keyword(rest, "DEFAULT") && tail_empty(&rest["DEFAULT".len()..]) {
        return RoleParse::Statement(RoleStatement::Set(SetRole::Default));
    }
    if starts_keyword(rest, "ALL EXCEPT") {
        let list = rest["ALL EXCEPT".len()..].trim_start();
        let Some((names, tail)) = parse_name_list(list) else {
            return RoleParse::NotRole;
        };
        if !tail_empty(tail) || names.is_empty() {
            return RoleParse::NotRole;
        }
        return RoleParse::Statement(RoleStatement::Set(SetRole::AllExcept(names)));
    }
    if starts_keyword(rest, "ALL") && tail_empty(&rest["ALL".len()..]) {
        return RoleParse::Statement(RoleStatement::Set(SetRole::All));
    }
    let Some((names, tail)) = parse_name_list(rest) else {
        return RoleParse::NotRole;
    };
    if !tail_empty(tail) || names.is_empty() {
        return RoleParse::NotRole;
    }
    RoleParse::Statement(RoleStatement::Set(SetRole::Named(names)))
}

fn parse_grant_or_revoke(sql: &str, grant: bool) -> RoleParse {
    let keyword = if grant { "GRANT" } else { "REVOKE" };
    let rest = sql[keyword.len()..].trim_start();
    let Some((roles, after_roles)) = parse_name_list(rest) else {
        return RoleParse::NotRole;
    };
    let conj = if grant { "TO" } else { "FROM" };
    if !starts_keyword(after_roles, conj) {
        return RoleParse::NotRole;
    }
    let grantee_sql = after_roles[conj.len()..].trim_start();
    let Some((grantees, tail)) = parse_name_list(grantee_sql) else {
        return RoleParse::NotRole;
    };
    if !tail_empty(tail) || roles.is_empty() || grantees.is_empty() {
        return RoleParse::NotRole;
    }
    if grant {
        RoleParse::Statement(RoleStatement::Grant { roles, grantees })
    } else {
        RoleParse::Statement(RoleStatement::Revoke { roles, grantees })
    }
}

fn has_top_level_on(sql: &str) -> bool {
    let upper = sql.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == b'\'' || c == b'"' || c == b'`' {
            quote = Some(c);
            i += 1;
            continue;
        }
        if c == b'O' && bytes.get(i + 1) == Some(&b'N') {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after = i + 2;
            let after_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn parse_name_list(input: &str) -> Option<(Vec<RoleName>, &str)> {
    let mut names = Vec::new();
    let mut rest = input.trim_start();
    if rest.is_empty() {
        return None;
    }
    loop {
        let (name, tail) = parse_role_name(rest)?;
        names.push(name);
        rest = tail.trim_start();
        if rest.starts_with(',') {
            rest = rest[1..].trim_start();
            continue;
        }
        break;
    }
    Some((names, rest))
}

fn parse_role_name(input: &str) -> Option<(RoleName, &str)> {
    let (user, rest) = parse_ident(input.trim_start())?;
    let rest_trim = rest.trim_start();
    if let Some(after_at) = rest_trim.strip_prefix('@') {
        let (host, tail) = parse_ident(after_at.trim_start())?;
        if host.is_empty() {
            return None;
        }
        return Some((RoleName { user, host }, tail));
    }
    Some((
        RoleName {
            user,
            host: "%".to_string(),
        },
        rest,
    ))
}

fn parse_ident(input: &str) -> Option<(String, &str)> {
    let input = input.trim_start();
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let quote = bytes[0];
    if quote == b'\'' || quote == b'"' || quote == b'`' {
        let mut value = String::new();
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i] == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    value.push(quote as char);
                    i += 2;
                    continue;
                }
                return Some((value, &input[i + 1..]));
            }
            value.push(bytes[i] as char);
            i += 1;
        }
        return None;
    }
    let end = input
        .find(|c: char| c.is_whitespace() || c == ',' || c == '@' || c == ';')
        .unwrap_or(input.len());
    if end == 0 {
        return None;
    }
    Some((input[..end].to_string(), &input[end..]))
}

fn role_name_from_object(name: &ObjectName) -> RoleName {
    let parts: Vec<String> = name.0.iter().map(|ident| ident.value.clone()).collect();
    match parts.as_slice() {
        [user, host] => RoleName {
            user: user.clone(),
            host: host.clone(),
        },
        [single] => role_name_from_text(single),
        _ => RoleName {
            user: parts.join("."),
            host: "%".to_string(),
        },
    }
}

fn role_name_from_ident(ident: &Ident) -> RoleName {
    role_name_from_text(&ident.value)
}

fn role_name_from_text(text: &str) -> RoleName {
    let text = text.trim_matches('`');
    if let Some((user, host)) = text.split_once('@') {
        RoleName {
            user: user.trim_matches('\'').to_string(),
            host: host.trim_matches('\'').to_string(),
        }
    } else {
        RoleName {
            user: text.to_string(),
            host: "%".to_string(),
        }
    }
}

fn starts_keyword(input: &str, keyword: &str) -> bool {
    let input = input.trim_start();
    let upper = input.to_ascii_uppercase();
    if !upper.starts_with(keyword) {
        return false;
    }
    let rest = input.get(keyword.len()..).unwrap_or("");
    rest.is_empty()
        || rest
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || c == ',' || c == ';')
}

fn tail_empty(tail: &str) -> bool {
    tail.trim().trim_end_matches(';').trim().is_empty()
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_schema_wildcard() {
        let sql = rewrite_grant_objects("GRANT SELECT ON rusql.* TO app");
        assert!(sql.contains("ALL TABLES IN SCHEMA rusql"));
    }

    #[test]
    fn create_role_parses_names_and_ignores_privilege_grant() {
        match parse_role_statement("CREATE ROLE r, 'web'@'localhost'") {
            RoleParse::Statement(RoleStatement::Create {
                names,
                if_not_exists,
            }) => {
                assert!(!if_not_exists);
                assert_eq!(names[0].user, "r");
                assert_eq!(names[0].host, "%");
                assert_eq!(names[1].host, "localhost");
            }
            other => panic!("expected create role, got {other:?}"),
        }
        match parse_role_statement("CREATE ROLE IF NOT EXISTS `app_read`") {
            RoleParse::Statement(RoleStatement::Create {
                names,
                if_not_exists,
            }) => {
                assert!(if_not_exists);
                assert_eq!(names[0].user, "app_read");
            }
            other => panic!("expected if not exists, got {other:?}"),
        }
        assert!(matches!(
            parse_role_statement("GRANT SELECT ON rusql.* TO r"),
            RoleParse::NotRole
        ));
        assert!(matches!(
            parse_role_statement("GRANT r TO 'app'@'%'"),
            RoleParse::Statement(RoleStatement::Grant { .. })
        ));
        assert!(matches!(
            parse_role_statement("REVOKE r FROM app"),
            RoleParse::Statement(RoleStatement::Revoke { .. })
        ));
        assert!(matches!(
            parse_role_statement("DROP ROLE IF EXISTS r"),
            RoleParse::Statement(RoleStatement::Drop {
                if_exists: true,
                ..
            })
        ));
        assert!(matches!(
            parse_role_statement("SET ROLE ALL EXCEPT r, 's'@'%'"),
            RoleParse::Statement(RoleStatement::Set(SetRole::AllExcept(_)))
        ));
        assert!(matches!(
            parse_role_statement("SET ROLE NONE"),
            RoleParse::Statement(RoleStatement::Set(SetRole::None))
        ));
        assert!(matches!(
            parse_role_statement("SET ROLE DEFAULT"),
            RoleParse::Statement(RoleStatement::Set(SetRole::Default))
        ));
        assert!(matches!(
            parse_role_statement("CREATE ROLE r; SELECT 1"),
            RoleParse::NotRole
        ));
    }
}

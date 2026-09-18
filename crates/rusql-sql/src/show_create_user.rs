//! Rewrite `SHOW CREATE USER` for sqlparser
//! (no `ShowCreateObject::User` in 0.53).

/// Internal virtual table the executor recognizes for `SHOW CREATE USER`.
pub const CREATE_USER_VIRTUAL_TABLE: &str = "__rusql_create_user";

/// Parsed `SHOW CREATE USER [account]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowCreateUser {
    pub user: Option<String>,
    pub host: Option<String>,
}

/// If `sql` is `SHOW CREATE USER …`, return the parsed form.
pub fn parse_show_create_user(sql: &str) -> Option<ShowCreateUser> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    let rest = skip_keyword(rest, "CREATE")?;
    let rest = skip_keyword(rest, "USER")?;
    let rest = rest.trim_start();
    if rest.is_empty() {
        return Some(ShowCreateUser {
            user: None,
            host: None,
        });
    }
    if let Some(after) = skip_keyword(rest, "CURRENT_USER") {
        let after = after.trim_start();
        let after = after.strip_prefix("()").unwrap_or(after).trim_start();
        if !after.is_empty() {
            return None;
        }
        return Some(ShowCreateUser {
            user: None,
            host: None,
        });
    }
    let (user, host, rest) = take_account(rest)?;
    if !rest.trim().is_empty() {
        return None;
    }
    Some(ShowCreateUser {
        user: Some(user),
        host: Some(host),
    })
}

/// Rewrite named `SHOW CREATE USER 'u'@'h'` to an internal SELECT.
pub fn rewrite_show_create_user(sql: &str) -> Option<String> {
    let parsed = parse_show_create_user(sql)?;
    let user = parsed.user?;
    let host = parsed.host?;
    Some(format!(
        "SELECT * FROM {CREATE_USER_VIRTUAL_TABLE} WHERE __user__ = '{}' AND __host__ = '{}'",
        escape_sql_string(&user),
        escape_sql_string(&host)
    ))
}

/// Rewrite `SHOW CREATE USER` / `CURRENT_USER` to the session account.
pub fn rewrite_show_create_user_current(sql: &str, user: &str, host: &str) -> Option<String> {
    let parsed = parse_show_create_user(sql)?;
    if parsed.user.is_some() {
        return None;
    }
    Some(format!(
        "SELECT * FROM {CREATE_USER_VIRTUAL_TABLE} WHERE __user__ = '{}' AND __host__ = '{}'",
        escape_sql_string(user),
        escape_sql_string(host)
    ))
}

fn skip_keyword<'a>(s: &'a str, keyword: &str) -> Option<&'a str> {
    let s = s.trim_start();
    if s.len() < keyword.len() {
        return None;
    }
    if !s.get(..keyword.len())?.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let after = &s[keyword.len()..];
    if after.is_empty()
        || after.starts_with(|c: char| c.is_ascii_whitespace())
        || after.starts_with('(')
        || after.starts_with('\'')
        || after.starts_with('`')
    {
        Some(after)
    } else {
        None
    }
}

fn take_account(s: &str) -> Option<(String, String, &str)> {
    let s = s.trim_start();
    if s.starts_with('\'') {
        let (user, host, end) = read_quoted_account(s.as_bytes(), 0)?;
        return Some((user, host, &s[end..]));
    }
    if let Some(rest) = s.strip_prefix('`') {
        let end = rest.find('`')?;
        let inner = &rest[..end];
        let after = &rest[end + 1..];
        if let Some((user, host)) = inner.split_once('@') {
            return Some((user.to_string(), host.to_string(), after));
        }
        if let Some(after_at) = after.trim_start().strip_prefix('@') {
            let (host, after) = take_ident_or_quoted(after_at.trim_start())?;
            return Some((inner.to_string(), host, after));
        }
        return Some((inner.to_string(), "%".into(), after));
    }
    if let Some((user, rest)) = take_ident(s) {
        let rest = rest.trim_start();
        if let Some(after_at) = rest.strip_prefix('@') {
            let (host, after) = take_ident_or_quoted(after_at.trim_start())?;
            return Some((user, host, after));
        }
        return Some((user, "%".into(), rest));
    }
    None
}

fn take_ident_or_quoted(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if s.starts_with('\'') {
        let (host, _, end) = read_quoted_account_host(s.as_bytes(), 0)?;
        return Some((host, &s[end..]));
    }
    take_ident(s)
}

fn take_ident(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('`') {
        let end = rest.find('`')?;
        return Some((rest[..end].to_string(), &rest[end + 1..]));
    }
    if let Some(rest) = s.strip_prefix('\'') {
        let end = rest.find('\'')?;
        return Some((rest[..end].to_string(), &rest[end + 1..]));
    }
    let n = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '%'))
        .unwrap_or(s.len());
    if n == 0 {
        return None;
    }
    Some((s[..n].to_string(), &s[n..]))
}

fn read_quoted_account(bytes: &[u8], start: usize) -> Option<(String, String, usize)> {
    if bytes.get(start)? != &b'\'' {
        return None;
    }
    let (user, mut i) = read_quoted(bytes, start + 1)?;
    if bytes.get(i)? != &b'@' {
        return None;
    }
    i += 1;
    if bytes.get(i)? != &b'\'' {
        return None;
    }
    let (host, end) = read_quoted(bytes, i + 1)?;
    Some((user, host, end))
}

fn read_quoted_account_host(bytes: &[u8], start: usize) -> Option<(String, String, usize)> {
    if bytes.get(start)? != &b'\'' {
        return None;
    }
    let (host, end) = read_quoted(bytes, start + 1)?;
    Some((String::new(), host, end))
}

fn read_quoted(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    let mut value = String::new();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                if bytes.get(i + 1) == Some(&b'\'') {
                    value.push('\'');
                    i += 2;
                    continue;
                }
                return Some((value, i + 1));
            }
            b => {
                value.push(b as char);
                i += 1;
            }
        }
    }
    None
}

fn escape_sql_string(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_quoted_and_current() {
        assert_eq!(
            parse_show_create_user("SHOW CREATE USER"),
            Some(ShowCreateUser {
                user: None,
                host: None,
            })
        );
        assert_eq!(
            parse_show_create_user("SHOW CREATE USER CURRENT_USER"),
            Some(ShowCreateUser {
                user: None,
                host: None,
            })
        );
        assert_eq!(
            parse_show_create_user("SHOW CREATE USER CURRENT_USER();"),
            Some(ShowCreateUser {
                user: None,
                host: None,
            })
        );
        assert_eq!(
            parse_show_create_user("SHOW CREATE USER 'app'@'%'"),
            Some(ShowCreateUser {
                user: Some("app".into()),
                host: Some("%".into()),
            })
        );
        assert_eq!(
            parse_show_create_user("show create user app@localhost"),
            Some(ShowCreateUser {
                user: Some("app".into()),
                host: Some("localhost".into()),
            })
        );
    }

    #[test]
    fn ignores_neighbors() {
        assert!(parse_show_create_user("SHOW CREATE FUNCTION f").is_none());
        assert!(parse_show_create_user("SHOW CREATE PROCEDURE p").is_none());
        assert!(parse_show_create_user("SHOW FUNCTION STATUS").is_none());
        assert!(parse_show_create_user("SHOW PROCEDURE STATUS").is_none());
        assert!(parse_show_create_user("CREATE USER 'app'@'%' IDENTIFIED BY 'x'").is_none());
        assert!(parse_show_create_user("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_named_and_current() {
        let sql = rewrite_show_create_user("SHOW CREATE USER 'app'@'%'").unwrap();
        assert!(sql.contains("__rusql_create_user"));
        assert!(sql.contains("__user__ = 'app'"));
        assert!(sql.contains("__host__ = '%'"));
        assert!(rewrite_show_create_user("SHOW CREATE USER").is_none());
        let sql = rewrite_show_create_user_current("SHOW CREATE USER", "root", "%").unwrap();
        assert!(sql.contains("__user__ = 'root'"));
        assert!(sql.contains("__host__ = '%'"));
    }
}

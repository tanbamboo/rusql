//! Rewrite `SHOW TRIGGERS` for sqlparser (no `Statement::ShowTriggers` in 0.53).

/// Internal virtual table the executor recognizes for `SHOW TRIGGERS`.
pub const TRIGGERS_VIRTUAL_TABLE: &str = "__rusql_show_triggers";

/// Parsed `SHOW TRIGGERS [{FROM|IN} db] [LIKE 'pattern']`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowTriggers {
    pub database: Option<String>,
    pub like: Option<String>,
}

/// If `sql` is `SHOW TRIGGERS …`, return the parsed form.
pub fn parse_show_triggers(sql: &str) -> Option<ShowTriggers> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    let rest = skip_keyword(rest, "TRIGGERS")?;

    let mut rest = rest.trim_start();
    let mut database = None;
    if let Some(after) = skip_keyword(rest, "FROM").or_else(|| skip_keyword(rest, "IN")) {
        let (name, after) = take_ident(after)?;
        if name.is_empty() {
            return None;
        }
        database = Some(name);
        rest = after.trim_start();
    }

    let mut like = None;
    if let Some(after) = skip_keyword(rest, "LIKE") {
        let (pattern, after) = take_string(after)?;
        like = Some(pattern);
        rest = after.trim_start();
    }

    if !rest.is_empty() {
        return None;
    }
    Some(ShowTriggers { database, like })
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_triggers(sql: &str) -> Option<String> {
    let parsed = parse_show_triggers(sql)?;
    let mut parts = Vec::new();
    if let Some(db) = parsed.database {
        parts.push(format!("__db__ = '{}'", escape_sql_string(&db)));
    }
    if let Some(pattern) = parsed.like {
        parts.push(format!("__like__ = '{}'", escape_sql_string(&pattern)));
    }
    if parts.is_empty() {
        Some(format!("SELECT * FROM {TRIGGERS_VIRTUAL_TABLE}"))
    } else {
        Some(format!(
            "SELECT * FROM {TRIGGERS_VIRTUAL_TABLE} WHERE {}",
            parts.join(" AND ")
        ))
    }
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
    if after.is_empty() || after.starts_with(|c: char| c.is_ascii_whitespace()) {
        Some(after)
    } else {
        None
    }
}

fn take_ident(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('`') {
        let end = rest.find('`')?;
        let name = rest[..end].to_string();
        Some((name, &rest[end + 1..]))
    } else {
        let n = s
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
            .unwrap_or(s.len());
        if n == 0 {
            return None;
        }
        Some((s[..n].to_string(), &s[n..]))
    }
}

fn take_string(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let rest = s.strip_prefix('\'')?;
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((i, ch)) = chars.next() {
        if ch == '\'' {
            if rest[i + ch.len_utf8()..].starts_with('\'') {
                out.push('\'');
                chars.next();
                continue;
            }
            return Some((out, &rest[i + ch.len_utf8()..]));
        }
        out.push(ch);
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
    fn parse_plain_and_like_and_from() {
        assert_eq!(
            parse_show_triggers("SHOW TRIGGERS"),
            Some(ShowTriggers {
                database: None,
                like: None,
            })
        );
        assert_eq!(
            parse_show_triggers("show triggers like 'tr%'"),
            Some(ShowTriggers {
                database: None,
                like: Some("tr%".into()),
            })
        );
        assert_eq!(
            parse_show_triggers("SHOW TRIGGERS FROM app_db LIKE 'tr%'"),
            Some(ShowTriggers {
                database: Some("app_db".into()),
                like: Some("tr%".into()),
            })
        );
        assert_eq!(
            parse_show_triggers("SHOW TRIGGERS IN `app_db`"),
            Some(ShowTriggers {
                database: Some("app_db".into()),
                like: None,
            })
        );
        assert_eq!(
            parse_show_triggers("SHOW TRIGGERS LIKE 'it''s'"),
            Some(ShowTriggers {
                database: None,
                like: Some("it's".into()),
            })
        );
    }

    #[test]
    fn ignores_show_create_trigger_and_where() {
        assert!(parse_show_triggers("SHOW CREATE TRIGGER t").is_none());
        assert!(parse_show_triggers("SHOW CREATE VIEW v").is_none());
        assert!(parse_show_triggers("SHOW CREATE TABLE t").is_none());
        assert!(parse_show_triggers("SHOW CREATE DATABASE rusql").is_none());
        assert!(parse_show_triggers("SHOW TABLE STATUS").is_none());
        assert!(parse_show_triggers("SHOW TABLES").is_none());
        assert!(parse_show_triggers("SHOW TRIGGERS WHERE Trigger = 't'").is_none());
        assert!(parse_show_triggers("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_to_internal_select() {
        assert_eq!(
            rewrite_show_triggers("SHOW TRIGGERS").as_deref(),
            Some("SELECT * FROM __rusql_show_triggers")
        );
        let sql = rewrite_show_triggers("SHOW TRIGGERS LIKE 'tr%'").unwrap();
        assert!(sql.contains("__rusql_show_triggers"));
        assert!(sql.contains("__like__ = 'tr%'"));
        let sql = rewrite_show_triggers("SHOW TRIGGERS FROM app_db").unwrap();
        assert!(sql.contains("__db__ = 'app_db'"));
    }
}

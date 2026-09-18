//! Rewrite `SHOW EVENTS` for sqlparser (no `Statement::ShowEvents` in 0.53).

/// Internal virtual table the executor recognizes for `SHOW EVENTS`.
pub const EVENTS_VIRTUAL_TABLE: &str = "__rusql_show_events";

/// Parsed `SHOW EVENTS [{FROM|IN} db] [LIKE 'pattern']`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowEvents {
    pub database: Option<String>,
    pub like: Option<String>,
}

/// If `sql` is `SHOW EVENTS …`, return the parsed form.
pub fn parse_show_events(sql: &str) -> Option<ShowEvents> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    let rest = skip_keyword(rest, "EVENTS")?;

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
    Some(ShowEvents { database, like })
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_events(sql: &str) -> Option<String> {
    let parsed = parse_show_events(sql)?;
    let mut parts = Vec::new();
    if let Some(db) = parsed.database {
        parts.push(format!("__db__ = '{}'", escape_sql_string(&db)));
    }
    if let Some(pattern) = parsed.like {
        parts.push(format!("__like__ = '{}'", escape_sql_string(&pattern)));
    }
    if parts.is_empty() {
        Some(format!("SELECT * FROM {EVENTS_VIRTUAL_TABLE}"))
    } else {
        Some(format!(
            "SELECT * FROM {EVENTS_VIRTUAL_TABLE} WHERE {}",
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
            parse_show_events("SHOW EVENTS"),
            Some(ShowEvents {
                database: None,
                like: None,
            })
        );
        assert_eq!(
            parse_show_events("show events like 'e%'"),
            Some(ShowEvents {
                database: None,
                like: Some("e%".into()),
            })
        );
        assert_eq!(
            parse_show_events("SHOW EVENTS FROM app_db LIKE 'e%'"),
            Some(ShowEvents {
                database: Some("app_db".into()),
                like: Some("e%".into()),
            })
        );
        assert_eq!(
            parse_show_events("SHOW EVENTS IN `app_db`"),
            Some(ShowEvents {
                database: Some("app_db".into()),
                like: None,
            })
        );
    }

    #[test]
    fn ignores_neighbors() {
        assert!(parse_show_events("SHOW CREATE EVENT e").is_none());
        assert!(parse_show_events("SHOW CREATE USER 'app'@'%'").is_none());
        assert!(parse_show_events("SHOW FUNCTION STATUS").is_none());
        assert!(parse_show_events("SHOW PROCEDURE STATUS").is_none());
        assert!(parse_show_events("SHOW TRIGGERS").is_none());
        assert!(parse_show_events("SHOW EVENTS WHERE Name = 'e'").is_none());
        assert!(parse_show_events("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_to_internal_select() {
        assert_eq!(
            rewrite_show_events("SHOW EVENTS").as_deref(),
            Some("SELECT * FROM __rusql_show_events")
        );
        let sql = rewrite_show_events("SHOW EVENTS FROM app_db LIKE 'e%'").unwrap();
        assert!(sql.contains("__rusql_show_events"));
        assert!(sql.contains("__db__ = 'app_db'"));
        assert!(sql.contains("__like__ = 'e%'"));
    }
}

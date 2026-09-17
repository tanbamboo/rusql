//! Rewrite `SHOW TABLE STATUS` for sqlparser (no `Statement::ShowTableStatus` in 0.53).

/// Internal virtual table the executor recognizes for `SHOW TABLE STATUS`.
pub const TABLE_STATUS_VIRTUAL_TABLE: &str = "__rusql_table_status";

/// Parsed `SHOW TABLE STATUS [{FROM|IN} db] [LIKE 'pattern']`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowTableStatus {
    pub database: Option<String>,
    pub like: Option<String>,
}

/// If `sql` is `SHOW TABLE STATUS …`, return the parsed form.
pub fn parse_show_table_status(sql: &str) -> Option<ShowTableStatus> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    let rest = skip_keyword(rest, "TABLE")?;
    let rest = skip_keyword(rest, "STATUS")?;

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
    Some(ShowTableStatus { database, like })
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_table_status(sql: &str) -> Option<String> {
    let parsed = parse_show_table_status(sql)?;
    let mut parts = Vec::new();
    if let Some(db) = parsed.database {
        parts.push(format!("__db__ = '{}'", escape_sql_string(&db)));
    }
    if let Some(pattern) = parsed.like {
        parts.push(format!("__like__ = '{}'", escape_sql_string(&pattern)));
    }
    if parts.is_empty() {
        Some(format!("SELECT * FROM {TABLE_STATUS_VIRTUAL_TABLE}"))
    } else {
        Some(format!(
            "SELECT * FROM {TABLE_STATUS_VIRTUAL_TABLE} WHERE {}",
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
            parse_show_table_status("SHOW TABLE STATUS"),
            Some(ShowTableStatus {
                database: None,
                like: None,
            })
        );
        assert_eq!(
            parse_show_table_status("show table status like 't%'"),
            Some(ShowTableStatus {
                database: None,
                like: Some("t%".into()),
            })
        );
        assert_eq!(
            parse_show_table_status("SHOW TABLE STATUS FROM app_db LIKE 't%'"),
            Some(ShowTableStatus {
                database: Some("app_db".into()),
                like: Some("t%".into()),
            })
        );
        assert_eq!(
            parse_show_table_status("SHOW TABLE STATUS IN `app_db`"),
            Some(ShowTableStatus {
                database: Some("app_db".into()),
                like: None,
            })
        );
        assert_eq!(
            parse_show_table_status("SHOW TABLE STATUS LIKE 'it''s'"),
            Some(ShowTableStatus {
                database: None,
                like: Some("it's".into()),
            })
        );
    }

    #[test]
    fn ignores_show_status_and_show_tables() {
        assert!(parse_show_table_status("SHOW STATUS").is_none());
        assert!(parse_show_table_status("SHOW SESSION STATUS").is_none());
        assert!(parse_show_table_status("SHOW TABLES").is_none());
        assert!(parse_show_table_status("SHOW ENGINE INNODB STATUS").is_none());
        assert!(parse_show_table_status("SHOW TABLE STATUS WHERE Name = 't'").is_none());
        assert!(parse_show_table_status("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_to_internal_select() {
        assert_eq!(
            rewrite_show_table_status("SHOW TABLE STATUS").as_deref(),
            Some("SELECT * FROM __rusql_table_status")
        );
        let sql = rewrite_show_table_status("SHOW TABLE STATUS LIKE 't%'").unwrap();
        assert!(sql.contains("__rusql_table_status"));
        assert!(sql.contains("__like__ = 't%'"));
        let sql = rewrite_show_table_status("SHOW TABLE STATUS FROM app_db").unwrap();
        assert!(sql.contains("__db__ = 'app_db'"));
    }
}

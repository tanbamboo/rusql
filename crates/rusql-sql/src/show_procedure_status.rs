//! Rewrite `SHOW PROCEDURE STATUS` for sqlparser
//! (no `Statement::ShowProcedureStatus` in 0.53).

/// Internal virtual table the executor recognizes for `SHOW PROCEDURE STATUS`.
pub const PROCEDURE_STATUS_VIRTUAL_TABLE: &str = "__rusql_procedure_status";

/// Parsed `SHOW PROCEDURE STATUS [LIKE 'pattern']`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowProcedureStatus {
    pub like: Option<String>,
}

/// If `sql` is `SHOW PROCEDURE STATUS …`, return the parsed form.
pub fn parse_show_procedure_status(sql: &str) -> Option<ShowProcedureStatus> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    let rest = skip_keyword(rest, "PROCEDURE")?;
    let rest = skip_keyword(rest, "STATUS")?;

    let mut rest = rest.trim_start();
    let mut like = None;
    if let Some(after) = skip_keyword(rest, "LIKE") {
        let (pattern, after) = take_string(after)?;
        like = Some(pattern);
        rest = after.trim_start();
    }

    if !rest.is_empty() {
        return None;
    }
    Some(ShowProcedureStatus { like })
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_procedure_status(sql: &str) -> Option<String> {
    let parsed = parse_show_procedure_status(sql)?;
    if let Some(pattern) = parsed.like {
        Some(format!(
            "SELECT * FROM {PROCEDURE_STATUS_VIRTUAL_TABLE} WHERE __like__ = '{}'",
            escape_sql_string(&pattern)
        ))
    } else {
        Some(format!("SELECT * FROM {PROCEDURE_STATUS_VIRTUAL_TABLE}"))
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
    fn parse_plain_and_like() {
        assert_eq!(
            parse_show_procedure_status("SHOW PROCEDURE STATUS"),
            Some(ShowProcedureStatus { like: None })
        );
        assert_eq!(
            parse_show_procedure_status("show procedure status like 'p%'"),
            Some(ShowProcedureStatus {
                like: Some("p%".into()),
            })
        );
        assert_eq!(
            parse_show_procedure_status("SHOW PROCEDURE STATUS LIKE 'it''s'"),
            Some(ShowProcedureStatus {
                like: Some("it's".into()),
            })
        );
    }

    #[test]
    fn ignores_neighbors() {
        assert!(parse_show_procedure_status("SHOW CREATE PROCEDURE p").is_none());
        assert!(parse_show_procedure_status("SHOW CREATE FUNCTION f").is_none());
        assert!(parse_show_procedure_status("SHOW CREATE TRIGGER t").is_none());
        assert!(parse_show_procedure_status("SHOW FUNCTION STATUS").is_none());
        assert!(parse_show_procedure_status("SHOW STATUS").is_none());
        assert!(parse_show_procedure_status("SHOW TRIGGERS").is_none());
        assert!(parse_show_procedure_status("SHOW PROCEDURE STATUS WHERE Name = 'p'").is_none());
        assert!(parse_show_procedure_status("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_to_internal_select() {
        assert_eq!(
            rewrite_show_procedure_status("SHOW PROCEDURE STATUS").as_deref(),
            Some("SELECT * FROM __rusql_procedure_status")
        );
        let sql = rewrite_show_procedure_status("SHOW PROCEDURE STATUS LIKE 'p%'").unwrap();
        assert!(sql.contains("__rusql_procedure_status"));
        assert!(sql.contains("__like__ = 'p%'"));
    }
}

//! Rewrite `SHOW WARNINGS` / `SHOW ERRORS` for sqlparser
//! (no `Statement::ShowWarnings` in 0.53).

/// Internal virtual table the executor recognizes for `SHOW WARNINGS` / `SHOW ERRORS`.
pub const WARNINGS_VIRTUAL_TABLE: &str = "__rusql_warnings";

/// If `sql` is `SHOW WARNINGS` or `SHOW ERRORS` (optional trailing `;`).
pub fn parse_show_warnings(sql: &str) -> Option<()> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    let rest = skip_keyword(rest, "WARNINGS").or_else(|| skip_keyword(rest, "ERRORS"))?;
    if !rest.trim().is_empty() {
        return None;
    }
    Some(())
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_warnings(sql: &str) -> Option<String> {
    parse_show_warnings(sql)?;
    Some(format!("SELECT * FROM {WARNINGS_VIRTUAL_TABLE}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_warnings_and_errors() {
        assert_eq!(parse_show_warnings("SHOW WARNINGS"), Some(()));
        assert_eq!(parse_show_warnings("show warnings;"), Some(()));
        assert_eq!(parse_show_warnings("SHOW ERRORS"), Some(()));
        assert_eq!(parse_show_warnings("show errors ;"), Some(()));
    }

    #[test]
    fn ignores_count_limit_and_neighbors() {
        assert!(parse_show_warnings("SHOW COUNT(*) WARNINGS").is_none());
        assert!(parse_show_warnings("SHOW COUNT(*) ERRORS").is_none());
        assert!(parse_show_warnings("SHOW WARNINGS LIMIT 1").is_none());
        assert!(parse_show_warnings("SHOW ERRORS LIMIT 1").is_none());
        assert!(parse_show_warnings("SHOW WARNINGS WHERE Code = 1").is_none());
        assert!(parse_show_warnings("SHOW CHARACTER SET").is_none());
        assert!(parse_show_warnings("SHOW ENGINES").is_none());
        assert!(parse_show_warnings("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_to_internal_select() {
        assert_eq!(
            rewrite_show_warnings("SHOW WARNINGS").as_deref(),
            Some("SELECT * FROM __rusql_warnings")
        );
        assert_eq!(
            rewrite_show_warnings("SHOW ERRORS").as_deref(),
            Some("SELECT * FROM __rusql_warnings")
        );
    }
}

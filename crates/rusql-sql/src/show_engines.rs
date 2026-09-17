//! Rewrite `SHOW ENGINES` / `SHOW STORAGE ENGINES` for sqlparser
//! (no `Statement::ShowEngines` in 0.53).

/// Internal virtual table the executor recognizes for `SHOW ENGINES`.
pub const ENGINES_VIRTUAL_TABLE: &str = "__rusql_engines";

/// If `sql` is `SHOW ENGINES` or `SHOW STORAGE ENGINES` (optional trailing `;`).
pub fn parse_show_engines(sql: &str) -> Option<()> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    let rest = skip_keyword(rest, "STORAGE").unwrap_or(rest.trim_start());
    let rest = skip_keyword(rest, "ENGINES")?;
    if !rest.trim().is_empty() {
        return None;
    }
    Some(())
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_engines(sql: &str) -> Option<String> {
    parse_show_engines(sql)?;
    Some(format!("SELECT * FROM {ENGINES_VIRTUAL_TABLE}"))
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
    fn parse_engines_and_storage_engines() {
        assert_eq!(parse_show_engines("SHOW ENGINES"), Some(()));
        assert_eq!(parse_show_engines("show engines;"), Some(()));
        assert_eq!(parse_show_engines("SHOW STORAGE ENGINES"), Some(()));
        assert_eq!(parse_show_engines("show storage engines ;"), Some(()));
    }

    #[test]
    fn ignores_show_engine_status_and_neighbors() {
        assert!(parse_show_engines("SHOW ENGINE INNODB STATUS").is_none());
        assert!(parse_show_engines("SHOW ENGINE MUSQL STATUS").is_none());
        assert!(parse_show_engines("SHOW TABLE STATUS").is_none());
        assert!(parse_show_engines("SHOW STATUS").is_none());
        assert!(parse_show_engines("SHOW ENGINES LIKE 'Inno%'").is_none());
        assert!(parse_show_engines("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_to_internal_select() {
        assert_eq!(
            rewrite_show_engines("SHOW ENGINES").as_deref(),
            Some("SELECT * FROM __rusql_engines")
        );
        assert_eq!(
            rewrite_show_engines("SHOW STORAGE ENGINES").as_deref(),
            Some("SELECT * FROM __rusql_engines")
        );
    }
}

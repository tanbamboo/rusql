//! Rewrite `SHOW BINARY LOGS` / `SHOW MASTER LOGS` for sqlparser
//! (no dedicated statement in 0.53).
//!
//! `SHOW BINLOG EVENTS` is rewritten separately (M123).

/// Internal virtual table the executor recognizes for `SHOW BINARY LOGS`.
pub const BINARY_LOGS_VIRTUAL_TABLE: &str = "__rusql_binary_logs";

/// If `sql` is `SHOW BINARY LOGS` or the MySQL synonym `SHOW MASTER LOGS`
/// (optional trailing `;`).
pub fn parse_show_binary_logs(sql: &str) -> Option<()> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    if let Some(rest) = skip_keyword(rest, "BINARY") {
        let rest = skip_keyword(rest, "LOGS")?;
        return rest.trim().is_empty().then_some(());
    }
    if let Some(rest) = skip_keyword(rest, "MASTER") {
        let rest = skip_keyword(rest, "LOGS")?;
        return rest.trim().is_empty().then_some(());
    }
    None
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_binary_logs(sql: &str) -> Option<String> {
    parse_show_binary_logs(sql)?;
    Some(format!("SELECT * FROM {BINARY_LOGS_VIRTUAL_TABLE}"))
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
    fn parse_binary_logs_and_master_logs() {
        assert_eq!(parse_show_binary_logs("SHOW BINARY LOGS"), Some(()));
        assert_eq!(parse_show_binary_logs("show binary logs;"), Some(()));
        assert_eq!(parse_show_binary_logs("SHOW MASTER LOGS"), Some(()));
        assert_eq!(parse_show_binary_logs("show master logs ;"), Some(()));
    }

    #[test]
    fn ignores_binlog_events_and_neighbors() {
        assert!(parse_show_binary_logs("SHOW BINLOG EVENTS").is_none());
        assert!(parse_show_binary_logs("SHOW MASTER STATUS").is_none());
        assert!(parse_show_binary_logs("SHOW BINARY LOG STATUS").is_none());
        assert!(parse_show_binary_logs("SHOW BINARY LOGS LIKE 'bin%'").is_none());
        assert!(parse_show_binary_logs("SHOW ENGINES").is_none());
        assert!(parse_show_binary_logs("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_binary_logs_to_internal_select() {
        assert_eq!(
            rewrite_show_binary_logs("SHOW BINARY LOGS").as_deref(),
            Some("SELECT * FROM __rusql_binary_logs")
        );
        assert_eq!(
            rewrite_show_binary_logs("SHOW MASTER LOGS").as_deref(),
            Some("SELECT * FROM __rusql_binary_logs")
        );
    }
}

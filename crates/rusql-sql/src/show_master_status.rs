//! Rewrite `SHOW MASTER STATUS` / `SHOW BINARY LOG STATUS` for sqlparser
//! (no dedicated statement in 0.53).

/// Internal virtual table the executor recognizes for master binlog status.
pub const MASTER_STATUS_VIRTUAL_TABLE: &str = "__rusql_master_status";

/// If `sql` is `SHOW MASTER STATUS` or MySQL 8.4+ `SHOW BINARY LOG STATUS`
/// (optional trailing `;`).
pub fn parse_show_master_status(sql: &str) -> Option<()> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    if let Some(rest) = skip_keyword(rest, "MASTER") {
        let rest = skip_keyword(rest, "STATUS")?;
        return rest.trim().is_empty().then_some(());
    }
    if let Some(rest) = skip_keyword(rest, "BINARY") {
        let rest = skip_keyword(rest, "LOG")?;
        let rest = skip_keyword(rest, "STATUS")?;
        return rest.trim().is_empty().then_some(());
    }
    None
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_master_status(sql: &str) -> Option<String> {
    parse_show_master_status(sql)?;
    Some(format!("SELECT * FROM {MASTER_STATUS_VIRTUAL_TABLE}"))
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
    fn parse_master_status_and_binary_log_status() {
        assert_eq!(parse_show_master_status("SHOW MASTER STATUS"), Some(()));
        assert_eq!(parse_show_master_status("show master status;"), Some(()));
        assert_eq!(parse_show_master_status("SHOW BINARY LOG STATUS"), Some(()));
        assert_eq!(
            parse_show_master_status("show binary log status ;"),
            Some(())
        );
    }

    #[test]
    fn ignores_binary_logs_and_neighbors() {
        assert!(parse_show_master_status("SHOW BINARY LOGS").is_none());
        assert!(parse_show_master_status("SHOW MASTER LOGS").is_none());
        assert!(parse_show_master_status("SHOW BINLOG EVENTS").is_none());
        assert!(parse_show_master_status("SHOW SLAVE STATUS").is_none());
        assert!(parse_show_master_status("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_master_status_to_internal_select() {
        assert_eq!(
            rewrite_show_master_status("SHOW MASTER STATUS").as_deref(),
            Some("SELECT * FROM __rusql_master_status")
        );
        assert_eq!(
            rewrite_show_master_status("SHOW BINARY LOG STATUS").as_deref(),
            Some("SELECT * FROM __rusql_master_status")
        );
    }
}

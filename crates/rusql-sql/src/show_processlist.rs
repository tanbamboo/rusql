//! Rewrite `SHOW PROCESSLIST` for sqlparser (not supported in sqlparser 0.53).

pub fn is_show_processlist(sql: &str) -> bool {
    let s = sql.trim().trim_end_matches(';').trim();
    s.eq_ignore_ascii_case("SHOW PROCESSLIST") || s.eq_ignore_ascii_case("SHOW FULL PROCESSLIST")
}

pub fn rewrite_show_processlist(sql: &str) -> Option<String> {
    if is_show_processlist(sql) {
        Some("SELECT * FROM __rusql_processlist".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_show_processlist() {
        assert!(is_show_processlist("SHOW PROCESSLIST"));
        assert!(!is_show_processlist("SHOW TABLES"));
    }
}

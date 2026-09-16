//! Rewrite `SET SESSION TRANSACTION` / `SET GLOBAL TRANSACTION` for sqlparser 0.53.
//!
//! sqlparser 0.53 parses `SET TRANSACTION …` but treats `SESSION` as a SET
//! modifier, so `SET SESSION TRANSACTION` fails. `SET GLOBAL TRANSACTION` is
//! rewritten to `SET @@global.transaction_isolation` so the executor can reject
//! it with errno 1229.

/// If `sql` is `SET SESSION TRANSACTION …` or `SET GLOBAL TRANSACTION …`, rewrite
/// so sqlparser / the executor can handle it.
///
/// Leaves `SET TRANSACTION …` (no SESSION/GLOBAL), `SET SESSION var = …`, and
/// non-SET statements untouched.
pub fn rewrite_set_transaction(sql: &str) -> Option<String> {
    let start = sql.find(|c: char| !c.is_whitespace())?;
    let rest = &sql[start..];
    if rest.len() < 3 || !rest[..3].eq_ignore_ascii_case("set") {
        return None;
    }
    let after_set = &rest[3..];
    let first = after_set.chars().next()?;
    if !first.is_whitespace() {
        return None;
    }
    let after_ws = after_set.trim_start();
    if let Some(after_session) = strip_keyword_word(after_ws, "SESSION") {
        let after_txn = strip_keyword_word(after_session.trim_start(), "TRANSACTION")?;
        let rest_txn = after_txn.trim_start();
        if rest_txn.is_empty() {
            return None;
        }
        return Some(format!("SET TRANSACTION {rest_txn}"));
    }
    if let Some(after_global) = strip_keyword_word(after_ws, "GLOBAL") {
        let after_txn = strip_keyword_word(after_global.trim_start(), "TRANSACTION")?;
        let after_txn = after_txn.trim_start();
        let (stmt, remainder) = split_first_statement(after_txn);
        let value = isolation_value_from_modes(stmt).unwrap_or("REPEATABLE-READ");
        return Some(format!(
            "SET @@global.transaction_isolation = '{value}'{remainder}"
        ));
    }
    None
}

fn strip_keyword_word<'a>(sql: &'a str, keyword: &str) -> Option<&'a str> {
    if sql.len() < keyword.len() || !sql[..keyword.len()].eq_ignore_ascii_case(keyword) {
        return None;
    }
    match sql[keyword.len()..].chars().next() {
        None => Some(""),
        Some(c) if c.is_whitespace() || c == ';' => Some(&sql[keyword.len()..]),
        Some(_) => None,
    }
}

fn split_first_statement(sql: &str) -> (&str, &str) {
    match sql.find(';') {
        Some(i) => (&sql[..i], &sql[i..]),
        None => (sql, ""),
    }
}

fn isolation_value_from_modes(modes: &str) -> Option<&'static str> {
    let rest = modes.trim_start();
    let after_iso = strip_keyword_word(rest, "ISOLATION")?;
    let after_level = strip_keyword_word(after_iso.trim_start(), "LEVEL")?;
    take_isolation_name(after_level.trim_start()).map(|(value, _)| value)
}

fn take_isolation_name(src: &str) -> Option<(&'static str, usize)> {
    let (w1, rest1) = next_word(src)?;
    if w1.eq_ignore_ascii_case("SERIALIZABLE") {
        return Some(("SERIALIZABLE", src.len() - rest1.len()));
    }
    if w1.eq_ignore_ascii_case("REPEATABLE") {
        let trimmed = rest1.trim_start();
        let (w2, rest2) = next_word(trimmed)?;
        if w2.eq_ignore_ascii_case("READ") {
            return Some(("REPEATABLE-READ", src.len() - rest2.len()));
        }
        return None;
    }
    if w1.eq_ignore_ascii_case("READ") {
        let trimmed = rest1.trim_start();
        let (w2, rest2) = next_word(trimmed)?;
        if w2.eq_ignore_ascii_case("UNCOMMITTED") {
            return Some(("READ-UNCOMMITTED", src.len() - rest2.len()));
        }
        if w2.eq_ignore_ascii_case("COMMITTED") {
            return Some(("READ-COMMITTED", src.len() - rest2.len()));
        }
    }
    None
}

fn next_word(sql: &str) -> Option<(&str, &str)> {
    let start = sql.find(|c: char| !c.is_whitespace())?;
    let rest = &sql[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == ';' || c == ',')
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    Some((&rest[..end], &rest[end..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_session_transaction_strips_session() {
        assert_eq!(
            rewrite_set_transaction("SET SESSION TRANSACTION ISOLATION LEVEL REPEATABLE READ")
                .as_deref(),
            Some("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        );
        assert_eq!(
            rewrite_set_transaction(
                "set session transaction isolation level read committed; SELECT @@tx_isolation"
            )
            .as_deref(),
            Some("SET TRANSACTION isolation level read committed; SELECT @@tx_isolation")
        );
    }

    #[test]
    fn rewrite_global_transaction_to_global_sysvar() {
        assert_eq!(
            rewrite_set_transaction("SET GLOBAL TRANSACTION ISOLATION LEVEL READ COMMITTED")
                .as_deref(),
            Some("SET @@global.transaction_isolation = 'READ-COMMITTED'")
        );
        assert_eq!(
            rewrite_set_transaction("SET GLOBAL TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .as_deref(),
            Some("SET @@global.transaction_isolation = 'SERIALIZABLE'")
        );
        assert_eq!(
            rewrite_set_transaction("SET GLOBAL TRANSACTION READ ONLY").as_deref(),
            Some("SET @@global.transaction_isolation = 'REPEATABLE-READ'")
        );
    }

    #[test]
    fn rewrite_skips_plain_transaction_and_session_vars() {
        assert!(
            rewrite_set_transaction("SET TRANSACTION ISOLATION LEVEL READ COMMITTED").is_none()
        );
        assert!(rewrite_set_transaction("SET SESSION autocommit = 1").is_none());
        assert!(
            rewrite_set_transaction("SET SESSION transaction_isolation = 'READ-COMMITTED'")
                .is_none()
        );
        assert!(rewrite_set_transaction("SET @@autocommit = 0").is_none());
        assert!(rewrite_set_transaction("SELECT 1").is_none());
    }
}

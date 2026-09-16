//! Rewrite `SET GLOBAL var = expr` so sqlparser 0.53 can parse it as `@@global.var`.

/// If `sql` is `SET GLOBAL …`, rewrite to `SET @@global.…`.
///
/// Leaves `SET GLOBAL TRANSACTION` and `SET GLOBAL NAMES` untouched (not this slice).
pub fn rewrite_set_global(sql: &str) -> Option<String> {
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
    if after_ws.len() < 6 || !after_ws[..6].eq_ignore_ascii_case("global") {
        return None;
    }
    let after_global = &after_ws[6..];
    if let Some(c) = after_global.chars().next() {
        if c.is_ascii_alphanumeric() || c == '_' {
            return None;
        }
    }
    let assignment = after_global.trim_start();
    if assignment.is_empty() {
        return None;
    }
    if keyword_at_start(assignment, "TRANSACTION") || keyword_at_start(assignment, "NAMES") {
        return None;
    }
    let name_part = if assignment.starts_with("@@") {
        assignment.trim_start_matches('@')
    } else {
        assignment
    };
    if name_part.len() >= 7 && name_part[..7].eq_ignore_ascii_case("global.") {
        Some(format!("SET @@{name_part}"))
    } else {
        Some(format!("SET @@global.{name_part}"))
    }
}

fn keyword_at_start(sql: &str, keyword: &str) -> bool {
    if sql.len() < keyword.len() || !sql[..keyword.len()].eq_ignore_ascii_case(keyword) {
        return false;
    }
    match sql[keyword.len()..].chars().next() {
        None => true,
        Some(c) => c.is_whitespace() || c == ';',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_set_global_plain_name() {
        assert_eq!(
            rewrite_set_global("SET GLOBAL autocommit = 0").as_deref(),
            Some("SET @@global.autocommit = 0")
        );
        assert_eq!(
            rewrite_set_global("set global autocommit=1").as_deref(),
            Some("SET @@global.autocommit=1")
        );
    }

    #[test]
    fn rewrite_set_global_at_at() {
        assert_eq!(
            rewrite_set_global("SET GLOBAL @@autocommit = 0").as_deref(),
            Some("SET @@global.autocommit = 0")
        );
        assert_eq!(
            rewrite_set_global("SET GLOBAL @@global.autocommit = 0").as_deref(),
            Some("SET @@global.autocommit = 0")
        );
    }

    #[test]
    fn rewrite_set_global_skips_session_and_names() {
        assert!(rewrite_set_global("SET SESSION autocommit = 1").is_none());
        assert!(rewrite_set_global("SET @@autocommit = 0").is_none());
        assert!(
            rewrite_set_global("SET GLOBAL TRANSACTION ISOLATION LEVEL REPEATABLE READ").is_none()
        );
        assert!(rewrite_set_global("SET GLOBAL NAMES utf8mb4").is_none());
    }
}

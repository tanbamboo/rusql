//! Rewrite `SHOW INDEX` / `SHOW INDEXES` / `SHOW KEYS` for sqlparser.

/// If `sql` is a MySQL-style `SHOW INDEX` variant, return the target table name.
pub fn parse_show_index_table(sql: &str) -> Option<String> {
    let s = sql.trim().trim_end_matches(';').trim();
    if s.len() < 12 {
        return None;
    }
    let upper = s.to_ascii_uppercase();
    if !upper.starts_with("SHOW ") {
        return None;
    }
    let rest = s[5..].trim_start();
    let upper_rest = rest.to_ascii_uppercase();
    let after_kind = if upper_rest.starts_with("INDEXES ") {
        &rest[8..]
    } else if upper_rest.starts_with("INDEX ") {
        &rest[6..]
    } else if upper_rest.starts_with("KEYS ") {
        &rest[5..]
    } else if upper_rest.starts_with("KEY ") {
        &rest[4..]
    } else {
        return None;
    };
    let rest = after_kind.trim_start();
    let upper_rest = rest.to_ascii_uppercase();
    let table_ref = if upper_rest.starts_with("FROM ") {
        rest[5..].trim()
    } else if upper_rest.starts_with("IN ") {
        rest[3..].trim()
    } else {
        return None;
    };
    let table_token = table_ref.split_whitespace().next()?;
    if table_token.is_empty() {
        return None;
    }
    Some(unquote_table_name(table_token))
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_index(sql: &str) -> Option<String> {
    let table = parse_show_index_table(sql)?;
    let escaped = table.replace('\'', "''");
    Some(format!(
        "SELECT * FROM __rusql_show_index WHERE __table__ = '{escaped}'"
    ))
}

fn unquote_table_name(token: &str) -> String {
    let token = token.trim();
    let bare = token.rsplit('.').next().unwrap_or(token).trim_matches('`');
    bare.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_show_index_from() {
        assert_eq!(
            parse_show_index_table("SHOW INDEX FROM users"),
            Some("users".into())
        );
    }

    #[test]
    fn parse_show_indexes_in_db_table() {
        assert_eq!(
            parse_show_index_table("SHOW INDEXES IN rusql.`stat_t`"),
            Some("stat_t".into())
        );
    }

    #[test]
    fn parse_show_keys() {
        assert_eq!(
            parse_show_index_table("SHOW KEYS FROM t;"),
            Some("t".into())
        );
    }

    #[test]
    fn rewrite_to_internal_select() {
        let sql = rewrite_show_index("SHOW INDEX FROM idx_t").unwrap();
        assert!(sql.contains("__rusql_show_index"));
        assert!(sql.contains("'idx_t'"));
    }

    #[test]
    fn ignores_select() {
        assert!(parse_show_index_table("SELECT 1").is_none());
    }
}

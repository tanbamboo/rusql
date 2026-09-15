//! Rewrite MySQL `SELECT SQL_CALC_FOUND_ROWS` (sqlparser 0.53 has no modifier).

/// Internal CTE name; executor treats this as the SQL_CALC_FOUND_ROWS flag.
pub const SQL_CALC_FOUND_ROWS_CTE: &str = "__rusql_sql_calc_found_rows";

const KEYWORD: &str = "SQL_CALC_FOUND_ROWS";

/// If `sql` contains `SQL_CALC_FOUND_ROWS`, strip the modifier and wrap a sentinel CTE.
pub fn rewrite_sql_calc_found_rows(sql: &str) -> Option<String> {
    let stripped = strip_sql_calc_found_rows(sql)?;
    Some(prepend_sentinel_cte(&stripped))
}

fn strip_sql_calc_found_rows(sql: &str) -> Option<String> {
    let upper = sql.to_ascii_uppercase();
    let idx = upper.find(KEYWORD)?;
    if idx > 0 {
        let before = sql.as_bytes()[idx - 1];
        if !before.is_ascii_whitespace() && before != b',' {
            return None;
        }
    }
    let after_idx = idx + KEYWORD.len();
    if after_idx < sql.len() {
        let after = sql.as_bytes()[after_idx];
        if after.is_ascii_alphanumeric() || after == b'_' {
            return None;
        }
    }
    let mut out = String::with_capacity(sql.len());
    out.push_str(&sql[..idx]);
    out.push_str(&sql[after_idx..]);
    Some(out)
}

fn prepend_sentinel_cte(sql: &str) -> String {
    let trimmed = sql.trim_start();
    let upper = trimmed.to_ascii_uppercase();
    if let Some(rest) = upper.strip_prefix("WITH ") {
        let original_rest = &trimmed[trimmed.len() - rest.len()..];
        let rest_upper = original_rest.to_ascii_uppercase();
        if let Some(after_rec) = rest_upper.strip_prefix("RECURSIVE ") {
            let original_after = &original_rest[original_rest.len() - after_rec.len()..];
            return format!(
                "WITH RECURSIVE {SQL_CALC_FOUND_ROWS_CTE} AS (SELECT 1), {original_after}"
            );
        }
        format!("WITH {SQL_CALC_FOUND_ROWS_CTE} AS (SELECT 1), {original_rest}")
    } else {
        format!("WITH {SQL_CALC_FOUND_ROWS_CTE} AS (SELECT 1) {trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn found_rows_rewrite_strips_modifier() {
        let out =
            rewrite_sql_calc_found_rows("SELECT SQL_CALC_FOUND_ROWS id FROM t LIMIT 1").unwrap();
        assert!(out.contains(SQL_CALC_FOUND_ROWS_CTE), "{out}");
        assert!(
            out.to_ascii_uppercase()
                .contains("SELECT  ID FROM T LIMIT 1"),
            "{out}"
        );
    }

    #[test]
    fn found_rows_rewrite_skips_plain_select() {
        assert!(rewrite_sql_calc_found_rows("SELECT id FROM t").is_none());
    }
}

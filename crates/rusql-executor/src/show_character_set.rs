//! Documented `SHOW CHARACTER SET` stubs (M89).
//!
//! Constant catalog for GUI/client probes. rusql does not convert wire encodings.
//! Aligned with M59/M62 utf8mb4 collations and M87 `SHOW TABLE STATUS` `Collation`.

use crate::info_schema::DEFAULT_COLLATION;
use crate::QueryResult;
use rusql_storage::Row;

pub(crate) const CHARACTER_SET_VIRTUAL_TABLE: &str = rusql_sql::CHARACTER_SET_VIRTUAL_TABLE;

pub(crate) const COLUMNS: [&str; 4] = ["Charset", "Description", "Default collation", "Maxlen"];

const STUB_CHARSET: &str = "utf8mb4";
const STUB_DESCRIPTION: &str = "UTF-8 Unicode";
const STUB_MAXLEN: &str = "4";

/// `SHOW CHARACTER SET` / `SHOW CHARSET` `[LIKE …]` over the documented stub catalog.
pub(crate) fn show_character_set(like: Option<&str>) -> QueryResult {
    let mut rows: Vec<Row> = vec![vec![
        STUB_CHARSET.to_string(),
        STUB_DESCRIPTION.to_string(),
        DEFAULT_COLLATION.to_string(),
        STUB_MAXLEN.to_string(),
    ]];
    if let Some(pattern) = like {
        rows.retain(|row| like_ci(&row[0], pattern));
    }
    QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows,
    }
}

fn like_ci(name: &str, pattern: &str) -> bool {
    like_bytes(
        name.to_ascii_lowercase().as_bytes(),
        pattern.to_ascii_lowercase().as_bytes(),
    )
}

fn like_bytes(name: &[u8], pattern: &[u8]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some((b'%', rest)) => {
            like_bytes(name, rest) || (!name.is_empty() && like_bytes(&name[1..], pattern))
        }
        Some((b'_', rest)) => !name.is_empty() && like_bytes(&name[1..], rest),
        Some((ch, rest)) => name.first() == Some(ch) && like_bytes(&name[1..], rest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_character_set_columns_utf8mb4_and_like() {
        match show_character_set(None) {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], STUB_CHARSET);
                assert_eq!(rows[0][1], STUB_DESCRIPTION);
                assert_eq!(rows[0][2], DEFAULT_COLLATION);
                assert_eq!(rows[0][3], STUB_MAXLEN);
            }
            other => panic!("expected rows, got {other:?}"),
        }

        match show_character_set(Some("utf8%")) {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], STUB_CHARSET);
            }
            other => panic!("expected LIKE rows, got {other:?}"),
        }
        match show_character_set(Some("no_such%")) {
            QueryResult::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE, got {other:?}"),
        }
    }
}

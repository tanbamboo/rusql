//! Documented `SHOW WARNINGS` / `SHOW ERRORS` stubs (M90).
//!
//! Empty diagnostic list until a later slice generates live warnings.
//! Columns match MySQL: `Level`, `Code`, `Message`.

use crate::QueryResult;

pub(crate) const WARNINGS_VIRTUAL_TABLE: &str = rusql_sql::WARNINGS_VIRTUAL_TABLE;

pub(crate) const COLUMNS: [&str; 3] = ["Level", "Code", "Message"];

/// `SHOW WARNINGS` / `SHOW ERRORS` over the documented empty stub list.
pub(crate) fn show_warnings() -> QueryResult {
    QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_warnings_columns_and_empty_rows() {
        match show_warnings() {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert!(rows.is_empty());
            }
            other => panic!("expected rows, got {other:?}"),
        }
    }
}

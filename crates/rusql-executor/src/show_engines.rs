//! Documented `SHOW ENGINES` stubs (M88).
//!
//! Constant catalog for GUI/client probes. rusql does not switch engines.
//! `SHOW ENGINE … STATUS` is not implemented here.

use crate::QueryResult;
use rusql_storage::Row;

pub(crate) const ENGINES_VIRTUAL_TABLE: &str = rusql_sql::ENGINES_VIRTUAL_TABLE;

pub(crate) const COLUMNS: [&str; 6] = [
    "Engine",
    "Support",
    "Comment",
    "Transactions",
    "XA",
    "Savepoints",
];

/// `(Engine, Support, Comment, Transactions, XA, Savepoints)`.
/// Documented stub subset — not the full MySQL 8.0 plugin list.
const STUBS: [[&str; 6]; 4] = [
    [
        "InnoDB",
        "DEFAULT",
        "Supports transactions, row-level locking, and foreign keys",
        "YES",
        "YES",
        "YES",
    ],
    [
        "MEMORY",
        "YES",
        "Hash based, stored in memory, useful for temporary tables",
        "NO",
        "NO",
        "NO",
    ],
    ["MyISAM", "YES", "MyISAM storage engine", "NO", "NO", "NO"],
    [
        "PERFORMANCE_SCHEMA",
        "YES",
        "Performance Schema",
        "NO",
        "NO",
        "NO",
    ],
];

/// `SHOW ENGINES` / `SHOW STORAGE ENGINES` over the documented stub catalog.
pub(crate) fn show_engines() -> QueryResult {
    let rows: Vec<Row> = STUBS
        .iter()
        .map(|row| row.iter().map(|s| (*s).to_string()).collect())
        .collect();
    QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_engines_columns_and_innodb_default() {
        match show_engines() {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(columns.len(), 6);
                let innodb = rows.iter().find(|r| r[0] == "InnoDB").expect("InnoDB row");
                assert_eq!(innodb[1], "DEFAULT");
                assert_eq!(innodb[3], "YES");
                assert_eq!(innodb[4], "YES");
                assert_eq!(innodb[5], "YES");
                assert!(rows.iter().any(|r| r[0] == "MEMORY" && r[1] == "YES"));
                assert!(rows.iter().any(|r| r[0] == "MyISAM" && r[1] == "YES"));
                assert!(rows
                    .iter()
                    .any(|r| r[0] == "PERFORMANCE_SCHEMA" && r[1] == "YES"));
                assert_eq!(rows.len(), STUBS.len());
            }
            other => panic!("expected rows, got {other:?}"),
        }
    }
}

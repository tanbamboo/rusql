//! `SHOW BINARY LOGS` listing (M122).
//!
//! Columns match MySQL (`Log_name`, `File_size`). Rows come from on-disk binlog
//! files created by M56–M74. An in-memory engine or missing binlog directory
//! returns the documented empty list.

use crate::QueryResult;
use rusql_storage::{Row, StorageEngine};

pub(crate) const BINARY_LOGS_VIRTUAL_TABLE: &str = rusql_sql::BINARY_LOGS_VIRTUAL_TABLE;

pub(crate) const COLUMNS: [&str; 2] = ["Log_name", "File_size"];

/// `SHOW BINARY LOGS` / `SHOW MASTER LOGS` over known binlog files.
pub(crate) fn show_binary_logs<E: StorageEngine>(engine: &E) -> QueryResult {
    let rows: Vec<Row> = engine
        .binary_log_files()
        .into_iter()
        .map(|(name, size)| vec![name, size.to_string()])
        .collect();
    QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_storage::HeapEngine;

    #[test]
    fn show_binary_logs_empty_without_files() {
        let engine = HeapEngine::new();
        match show_binary_logs(&engine) {
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

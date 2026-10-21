//! `SHOW MASTER STATUS` / `SHOW BINARY LOG STATUS` (M175).
//!
//! Columns match MySQL (`File`, `Position`, `Binlog_Do_DB`, `Binlog_Ignore_DB`,
//! `Executed_Gtid_Set`). `File` and `Position` reflect the current binlog file
//! on disk; `Executed_Gtid_Set` reflects persisted `@@gtid_executed` (M176).

use crate::QueryResult;
use rusql_storage::StorageEngine;

pub(crate) const MASTER_STATUS_VIRTUAL_TABLE: &str = rusql_sql::MASTER_STATUS_VIRTUAL_TABLE;

pub(crate) const COLUMNS: [&str; 5] = [
    "File",
    "Position",
    "Binlog_Do_DB",
    "Binlog_Ignore_DB",
    "Executed_Gtid_Set",
];

/// Live master binlog status from storage (empty when no binlog files).
pub(crate) fn show_master_status<E: StorageEngine>(engine: &E) -> QueryResult {
    let columns: Vec<String> = COLUMNS.iter().map(|s| (*s).to_string()).collect();
    let Some((file, position)) = engine.master_binlog_status() else {
        return QueryResult::Rows {
            columns,
            rows: Vec::new(),
        };
    };
    let executed = engine.gtid_executed();
    QueryResult::Rows {
        columns,
        rows: vec![vec![
            file,
            position.to_string(),
            String::new(),
            String::new(),
            executed,
        ]],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_storage::HeapEngine;

    #[test]
    fn show_master_status_empty_without_files() {
        let engine = HeapEngine::new();
        match show_master_status(&engine) {
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

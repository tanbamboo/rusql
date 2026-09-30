//! `SHOW BINLOG EVENTS` listing (M123).
//!
//! Columns match MySQL (`Log_name`, `Pos`, `Event_type`, `Server_id`,
//! `End_log_pos`, `Info`). Rows come from on-disk binlog files created by
//! M56–M74. An in-memory engine or missing binlog directory returns the
//! documented empty list. Unknown event types use `Event_type` `Unknown` and
//! an empty Info stub. `SHOW BINARY LOGS` is unchanged (M122).

use crate::{ExecError, QueryResult};
use rusql_storage::{Row, StorageEngine};

pub(crate) const BINLOG_EVENTS_VIRTUAL_TABLE: &str = rusql_sql::BINLOG_EVENTS_VIRTUAL_TABLE;

pub(crate) const COLUMNS: [&str; 6] = [
    "Log_name",
    "Pos",
    "Event_type",
    "Server_id",
    "End_log_pos",
    "Info",
];

/// `SHOW BINLOG EVENTS [IN log] [FROM pos]` over a known binlog file.
pub(crate) fn show_binlog_events<E: StorageEngine>(
    engine: &E,
    log_name: Option<&str>,
    from_pos: Option<u32>,
) -> Result<QueryResult, ExecError> {
    if let Some(name) = log_name {
        if !engine.binary_log_files().iter().any(|(n, _)| n == name) {
            return Err(ExecError::Mysql {
                code: 1220,
                message: rusql_i18n::messages::sql_binlog_log_not_found(name),
            });
        }
    }
    let rows: Vec<Row> = engine
        .binlog_events(log_name, from_pos)
        .into_iter()
        .map(|e| {
            vec![
                e.log_name,
                e.pos.to_string(),
                e.event_type,
                e.server_id.to_string(),
                e.end_log_pos.to_string(),
                e.info,
            ]
        })
        .collect();
    Ok(QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_storage::HeapEngine;

    #[test]
    fn show_binlog_events_empty_without_files() {
        let engine = HeapEngine::new();
        match show_binlog_events(&engine, None, None).unwrap() {
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

    #[test]
    fn unknown_in_log_is_errno_1220() {
        let engine = HeapEngine::new();
        match show_binlog_events(&engine, Some("binlog.000001"), None) {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, 1220);
                assert!(message.contains("binlog.000001"));
            }
            other => panic!("expected errno 1220, got {other:?}"),
        }
    }
}

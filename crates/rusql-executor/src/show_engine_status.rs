//! `SHOW ENGINE … STATUS` (M131 columns, M193 live snapshot).
//!
//! `SHOW ENGINE INNODB STATUS` returns MySQL-shaped columns. The `Status`
//! cell keeps an InnoDB-monitor header plus rusql `TRANSACTIONS` and `LOCKS`
//! sections filled from the open-transaction registry. Section names are
//! monitor-format tokens in every locale (the same role as general-log
//! `Connect` / `Query`). This is not a buffer-pool report and not Oracle
//! mutex counters. Unknown engine names are errno 1286.
//! `SHOW ENGINES` is unchanged (M88).

use crate::{ExecError, QueryResult};
use rusql_storage::{OpenTxn, Row, TxnLockNote};

pub(crate) const ENGINE_STATUS_VIRTUAL_TABLE: &str = rusql_sql::ENGINE_STATUS_VIRTUAL_TABLE;

pub(crate) const COLUMNS: [&str; 3] = ["Type", "Name", "Status"];

/// MySQL `ER_UNKNOWN_STORAGE_ENGINE`.
const ER_UNKNOWN_STORAGE_ENGINE: u16 = 1286;

/// `SHOW ENGINE engine STATUS` (InnoDB compatibility name only).
pub(crate) fn show_engine_status(engine: &str) -> Result<QueryResult, ExecError> {
    if !engine.eq_ignore_ascii_case("InnoDB") {
        return Err(ExecError::Mysql {
            code: ER_UNKNOWN_STORAGE_ENGINE,
            message: rusql_i18n::messages::sql_unknown_storage_engine(engine),
        });
    }
    let status = format_innodb_monitor(&rusql_storage::open_txn_snapshot());
    Ok(QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows: vec![vec!["InnoDB".to_string(), String::new(), status] as Row],
    })
}

/// MySQL-shaped monitor header plus the rusql transaction and lock snapshot.
///
/// `INNODB MONITOR OUTPUT`, `TRANSACTIONS`, `LOCKS`, `---TRANSACTION`,
/// `MySQL thread id`, `RECORD LOCK`, and `GAP LOCK` are format tokens.
fn format_innodb_monitor(txns: &[OpenTxn]) -> String {
    let mut out = String::new();
    out.push_str("=====================================\n");
    out.push_str("INNODB MONITOR OUTPUT\n");
    out.push_str("=====================================\n");
    out.push_str("------------\n");
    out.push_str("TRANSACTIONS\n");
    out.push_str("------------\n");
    for txn in txns {
        out.push_str(&format!("---TRANSACTION {}, ACTIVE\n", txn.trx_id));
        out.push_str(&format!("MySQL thread id {}\n", txn.connection_id));
    }
    out.push_str("--------\n");
    out.push_str("LOCKS\n");
    out.push_str("--------\n");
    for txn in txns {
        for lock in &txn.locks {
            match lock {
                TxnLockNote::Record { table, pk } => {
                    out.push_str(&format!(
                        "RECORD LOCK table {table} trx id {} lock_mode X pk {pk}\n",
                        txn.trx_id
                    ));
                }
                TxnLockNote::Gap { table, low, high } => {
                    out.push_str(&format!(
                        "GAP LOCK table {table} trx id {} lock_mode X low {low} high {high}\n",
                        txn.trx_id
                    ));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_engine_innodb_status_header_row() {
        match show_engine_status("InnoDB").unwrap() {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "InnoDB");
                assert_eq!(rows[0][1], "");
                assert!(rows[0][2].contains("INNODB MONITOR OUTPUT"));
                assert!(rows[0][2].contains("TRANSACTIONS"));
                assert!(rows[0][2].contains("LOCKS"));
                assert!(!rows[0][2].contains("BUFFER POOL"));
            }
            other => panic!("expected rows, got {other:?}"),
        }
        match show_engine_status("innodb").unwrap() {
            QueryResult::Rows { rows, .. } => assert_eq!(rows[0][0], "InnoDB"),
            other => panic!("expected rows, got {other:?}"),
        }
    }

    #[test]
    fn unknown_engine_is_errno_1286() {
        match show_engine_status("MUSQL") {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, 1286);
                assert!(message.contains("MUSQL"));
            }
            other => panic!("expected errno 1286, got {other:?}"),
        }
    }
}

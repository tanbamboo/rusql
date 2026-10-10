//! Documented `SHOW ENGINE … STATUS` stub (M131).
//!
//! `SHOW ENGINE INNODB STATUS` returns MySQL-shaped columns with a
//! non-empty i18n stub `Status` cell. This is not live mutex/lock
//! statistics (M193). Unknown engine names are errno 1286.
//! `SHOW ENGINES` is unchanged (M88).

use crate::{ExecError, QueryResult};
use rusql_storage::Row;

pub(crate) const ENGINE_STATUS_VIRTUAL_TABLE: &str = rusql_sql::ENGINE_STATUS_VIRTUAL_TABLE;

pub(crate) const COLUMNS: [&str; 3] = ["Type", "Name", "Status"];

/// MySQL `ER_UNKNOWN_STORAGE_ENGINE`.
const ER_UNKNOWN_STORAGE_ENGINE: u16 = 1286;

/// `SHOW ENGINE engine STATUS` documented stub (InnoDB only).
pub(crate) fn show_engine_status(engine: &str) -> Result<QueryResult, ExecError> {
    if !engine.eq_ignore_ascii_case("InnoDB") {
        return Err(ExecError::Mysql {
            code: ER_UNKNOWN_STORAGE_ENGINE,
            message: rusql_i18n::messages::sql_unknown_storage_engine(engine),
        });
    }
    let status = rusql_i18n::messages::sql_show_engine_innodb_status_stub();
    Ok(QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows: vec![vec!["InnoDB".to_string(), String::new(), status] as Row],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_engine_innodb_status_stub_row() {
        match show_engine_status("InnoDB").unwrap() {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "InnoDB");
                assert_eq!(rows[0][1], "");
                assert!(!rows[0][2].is_empty());
                assert_eq!(
                    rows[0][2],
                    rusql_i18n::messages::sql_show_engine_innodb_status_stub()
                );
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

//! Documented `SHOW EVENTS` stubs (M101).
//!
//! Empty catalog until `CREATE EVENT` exists. Columns match MySQL 8.0
//! `SHOW EVENTS` order. Unknown `FROM`/`IN` databases are errno 1049.

use crate::{ExecError, QueryResult};
use rusql_core::Session;
use rusql_storage::StorageEngine;

pub(crate) const EVENTS_VIRTUAL_TABLE: &str = rusql_sql::EVENTS_VIRTUAL_TABLE;

/// MySQL 8.0 `SHOW EVENTS` column order.
pub(crate) const COLUMNS: [&str; 15] = [
    "Db",
    "Name",
    "Definer",
    "Time zone",
    "Type",
    "Execute at",
    "Interval value",
    "Interval field",
    "Starts",
    "Ends",
    "Status",
    "Originator",
    "character_set_client",
    "collation_connection",
    "Database Collation",
];

/// `SHOW EVENTS [{FROM|IN} db] [LIKE …]` over an empty event catalog.
pub(crate) fn show_events<E: StorageEngine>(
    engine: &E,
    session: &Session,
    database: Option<&str>,
    _like: Option<&str>,
) -> Result<QueryResult, ExecError> {
    let db = database.unwrap_or(session.database.as_str());
    if !engine.list_databases().iter().any(|d| d == db) {
        return Err(ExecError::Mysql {
            code: 1049,
            message: rusql_i18n::messages::storage_database_not_found(db),
        });
    }
    Ok(QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_storage::HeapEngine;

    #[test]
    fn show_events_empty_columns_like_and_unknown_db() {
        let engine = HeapEngine::new();
        let session = Session::new(1, "root");

        match show_events(&engine, &session, None, None) {
            Ok(QueryResult::Rows { columns, rows }) => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(columns[0], "Db");
                assert_eq!(columns[1], "Name");
                assert_eq!(columns[10], "Status");
                assert_eq!(columns[12], "character_set_client");
                assert_eq!(columns[13], "collation_connection");
                assert_eq!(columns[14], "Database Collation");
                assert!(rows.is_empty());
            }
            other => panic!("expected empty rows, got {other:?}"),
        }

        match show_events(&engine, &session, None, Some("e%")) {
            Ok(QueryResult::Rows { rows, .. }) => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE, got {other:?}"),
        }

        match show_events(&engine, &session, Some("no_such_db"), None) {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, 1049);
                assert!(message.contains("no_such_db"));
            }
            other => panic!("expected errno 1049, got {other:?}"),
        }
    }
}

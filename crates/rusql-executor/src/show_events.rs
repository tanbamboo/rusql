//! Documented `SHOW EVENTS` stubs (M101 / M102).
//!
//! Catalog `Db` / `Name` / schedule cells from `EventMeta` when present.
//! Other cells are stubs — not live Definer / last-executed / charset catalogs.
//! Unknown `FROM`/`IN` databases are errno 1049.

use crate::info_schema::{DEFAULT_CHARSET, DEFAULT_COLLATION};
use crate::{ExecError, QueryResult};
use rusql_core::{EventMeta, Session};
use rusql_storage::{Row, StorageEngine};

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

const STUB_DEFINER: &str = "root@%";
const STUB_TIME_ZONE: &str = "SYSTEM";
const STUB_ORIGINATOR: &str = "1";

/// `SHOW EVENTS [{FROM|IN} db] [LIKE …]` over the event catalog.
pub(crate) fn show_events<E: StorageEngine>(
    engine: &E,
    session: &Session,
    database: Option<&str>,
    like: Option<&str>,
) -> Result<QueryResult, ExecError> {
    let db = database.unwrap_or(session.database.as_str());
    if !engine.list_databases().iter().any(|d| d == db) {
        return Err(ExecError::Mysql {
            code: 1049,
            message: rusql_i18n::messages::storage_database_not_found(db),
        });
    }
    let mut events: Vec<&EventMeta> = session
        .catalog
        .iter_events()
        .filter(|e| e.schema.eq_ignore_ascii_case(db))
        .collect();
    if let Some(pattern) = like {
        events.retain(|e| like_ci(&e.name, pattern));
    }
    events.sort_by(|a, b| a.schema.cmp(&b.schema).then_with(|| a.name.cmp(&b.name)));
    Ok(QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows: events.into_iter().map(event_row).collect(),
    })
}

fn event_row(meta: &EventMeta) -> Row {
    vec![
        meta.schema.clone(),
        meta.name.clone(),
        STUB_DEFINER.to_string(),
        STUB_TIME_ZONE.to_string(),
        meta.schedule_type.clone(),
        meta.execute_at.clone().unwrap_or_default(),
        meta.interval_value.clone().unwrap_or_default(),
        meta.interval_field.clone().unwrap_or_default(),
        String::new(),
        String::new(),
        meta.status.clone(),
        STUB_ORIGINATOR.to_string(),
        DEFAULT_CHARSET.to_string(),
        DEFAULT_COLLATION.to_string(),
        DEFAULT_COLLATION.to_string(),
    ]
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
    use rusql_core::DEFAULT_SCHEMA;
    use rusql_storage::HeapEngine;

    fn session_with_event(name: &str) -> Session {
        let mut session = Session::new(1, "root");
        session.catalog.create_event(EventMeta {
            schema: DEFAULT_SCHEMA.into(),
            name: name.into(),
            schedule_type: "ONE TIME".into(),
            execute_at: Some("2038-01-01 00:00:00".into()),
            interval_value: None,
            interval_field: None,
            status: "ENABLED".into(),
            body: "SELECT 1".into(),
        });
        session
    }

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

    #[test]
    fn show_events_catalog_row_and_like() {
        let engine = HeapEngine::new();
        let session = session_with_event("e");

        match show_events(&engine, &session, None, None) {
            Ok(QueryResult::Rows { rows, .. }) => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], DEFAULT_SCHEMA);
                assert_eq!(rows[0][1], "e");
                assert_eq!(rows[0][2], STUB_DEFINER);
                assert_eq!(rows[0][4], "ONE TIME");
                assert_eq!(rows[0][5], "2038-01-01 00:00:00");
                assert_eq!(rows[0][10], "ENABLED");
            }
            other => panic!("expected catalog row, got {other:?}"),
        }

        match show_events(&engine, &session, None, Some("e%")) {
            Ok(QueryResult::Rows { rows, .. }) => assert_eq!(rows.len(), 1),
            other => panic!("expected LIKE match, got {other:?}"),
        }
        match show_events(&engine, &session, None, Some("no_such%")) {
            Ok(QueryResult::Rows { rows, .. }) => assert!(rows.is_empty()),
            other => panic!("expected unmatched LIKE, got {other:?}"),
        }
    }
}

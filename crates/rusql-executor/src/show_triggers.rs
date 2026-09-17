//! Documented `SHOW TRIGGERS` stubs (M93).
//!
//! Catalog `Trigger` / `Event` / `Table` / `Timing` / `Statement` from M48 `TriggerMeta`.
//! Other cells are stubs — not live DEFINER / sql_mode / charset catalogs.

use crate::info_schema::{DEFAULT_CHARSET, DEFAULT_COLLATION};
use crate::{ExecError, QueryResult};
use rusql_core::{Session, TriggerEvent, TriggerMeta, TriggerTiming};
use rusql_storage::{Row, StorageEngine};

pub(crate) const TRIGGERS_VIRTUAL_TABLE: &str = rusql_sql::TRIGGERS_VIRTUAL_TABLE;

pub(crate) const COLUMNS: [&str; 11] = [
    "Trigger",
    "Event",
    "Table",
    "Statement",
    "Timing",
    "Created",
    "sql_mode",
    "Definer",
    "character_set_client",
    "collation_connection",
    "Database Collation",
];

const STUB_DEFINER: &str = "root@%";

/// `SHOW TRIGGERS [{FROM|IN} db] [LIKE …]` over catalog triggers in `db` (or the session database).
pub(crate) fn show_triggers<E: StorageEngine>(
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
    let mut triggers: Vec<&TriggerMeta> = session
        .catalog
        .iter_triggers()
        .filter(|t| t.schema == db)
        .collect();
    if let Some(pattern) = like {
        triggers.retain(|t| like_ci(&t.name, pattern));
    }
    triggers.sort_by(|a, b| a.table.cmp(&b.table).then_with(|| a.name.cmp(&b.name)));
    let rows: Vec<Row> = triggers.into_iter().map(trigger_row).collect();
    Ok(QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows,
    })
}

fn trigger_row(meta: &TriggerMeta) -> Row {
    vec![
        meta.name.clone(),
        event_name(meta.event).to_string(),
        meta.table.clone(),
        meta.body.join("; "),
        timing_name(meta.timing).to_string(),
        String::new(),
        String::new(),
        STUB_DEFINER.to_string(),
        DEFAULT_CHARSET.to_string(),
        DEFAULT_COLLATION.to_string(),
        DEFAULT_COLLATION.to_string(),
    ]
}

fn event_name(event: TriggerEvent) -> &'static str {
    match event {
        TriggerEvent::Insert => "INSERT",
        TriggerEvent::Update => "UPDATE",
        TriggerEvent::Delete => "DELETE",
    }
}

fn timing_name(timing: TriggerTiming) -> &'static str {
    match timing {
        TriggerTiming::Before => "BEFORE",
        TriggerTiming::After => "AFTER",
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
    use rusql_core::{ColumnDef, TableMeta, DEFAULT_SCHEMA};
    use rusql_storage::HeapEngine;

    fn session_with_trigger(name: &str, table: &str) -> (HeapEngine, Session) {
        let engine = HeapEngine::new();
        let mut session = Session::new(1, "root");
        session.catalog.create_table(TableMeta {
            name: table.into(),
            schema: DEFAULT_SCHEMA.into(),
            columns: vec![ColumnDef::new("id", "INT")],
            auto_increment_next: None,
            ..Default::default()
        });
        session.catalog.create_trigger(TriggerMeta {
            schema: DEFAULT_SCHEMA.into(),
            table: table.into(),
            name: name.into(),
            timing: TriggerTiming::Before,
            event: TriggerEvent::Insert,
            body: vec!["SET NEW.id = NEW.id".into()],
        });
        (engine, session)
    }

    #[test]
    fn show_triggers_columns_catalog_like_and_unknown_db() {
        let (engine, session) = session_with_trigger("tr_src", "src");

        match show_triggers(&engine, &session, None, None).unwrap() {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "tr_src");
                assert_eq!(rows[0][1], "INSERT");
                assert_eq!(rows[0][2], "src");
                assert_eq!(rows[0][3], "SET NEW.id = NEW.id");
                assert_eq!(rows[0][4], "BEFORE");
                assert_eq!(rows[0][5], "");
                assert_eq!(rows[0][6], "");
                assert_eq!(rows[0][7], STUB_DEFINER);
                assert_eq!(rows[0][8], DEFAULT_CHARSET);
                assert_eq!(rows[0][9], DEFAULT_COLLATION);
                assert_eq!(rows[0][10], DEFAULT_COLLATION);
            }
            other => panic!("expected rows, got {other:?}"),
        }

        match show_triggers(&engine, &session, None, Some("tr_%")).unwrap() {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "tr_src");
            }
            other => panic!("expected LIKE rows, got {other:?}"),
        }
        match show_triggers(&engine, &session, None, Some("no_such%")).unwrap() {
            QueryResult::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE, got {other:?}"),
        }

        match show_triggers(&engine, &session, Some("missing_db"), None) {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, 1049);
                assert!(message.contains("missing_db"));
            }
            other => panic!("expected errno 1049, got {other:?}"),
        }
    }
}

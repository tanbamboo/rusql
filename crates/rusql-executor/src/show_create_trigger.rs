//! Documented `SHOW CREATE TRIGGER` stubs (M94).
//!
//! `SQL Original Statement` is reconstructed from catalog `TriggerMeta`.
//! Other cells are stubs — not live DEFINER / sql_mode / charset catalogs.

use crate::info_schema::{DEFAULT_CHARSET, DEFAULT_COLLATION};
use crate::{ExecError, QueryResult};
use rusql_core::{Session, TriggerEvent, TriggerMeta, TriggerTiming};

pub(crate) const CREATE_TRIGGER_VIRTUAL_TABLE: &str = rusql_sql::CREATE_TRIGGER_VIRTUAL_TABLE;

/// MySQL 8.0 `SHOW CREATE TRIGGER` column order.
pub(crate) const COLUMNS: [&str; 7] = [
    "Trigger",
    "sql_mode",
    "SQL Original Statement",
    "character_set_client",
    "collation_connection",
    "Database Collation",
    "Created",
];

/// MySQL `ER_TRG_DOES_NOT_EXIST`.
const ER_TRG_DOES_NOT_EXIST: u16 = 1360;

/// `SHOW CREATE TRIGGER [db.]name` reconstructed from the session catalog.
pub(crate) fn show_create_trigger(
    session: &Session,
    database: Option<&str>,
    name: &str,
) -> Result<QueryResult, ExecError> {
    let db = database.unwrap_or(session.database.as_str());
    let meta = find_trigger(session, db, name).ok_or_else(|| ExecError::Mysql {
        code: ER_TRG_DOES_NOT_EXIST,
        message: rusql_i18n::messages::trigger_not_found(name),
    })?;
    Ok(QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows: vec![trigger_row(meta)],
    })
}

fn find_trigger<'a>(session: &'a Session, schema: &str, name: &str) -> Option<&'a TriggerMeta> {
    session.catalog.get_trigger(schema, name).or_else(|| {
        session
            .catalog
            .iter_triggers()
            .find(|t| t.schema.eq_ignore_ascii_case(schema) && t.name.eq_ignore_ascii_case(name))
    })
}

fn trigger_row(meta: &TriggerMeta) -> Vec<String> {
    vec![
        meta.name.clone(),
        String::new(),
        original_statement(meta),
        DEFAULT_CHARSET.to_string(),
        DEFAULT_COLLATION.to_string(),
        DEFAULT_COLLATION.to_string(),
        String::new(),
    ]
}

fn original_statement(meta: &TriggerMeta) -> String {
    let name = meta.name.replace('`', "``");
    let table = meta.table.replace('`', "``");
    format!(
        "CREATE TRIGGER `{name}` {} {} ON `{table}` FOR EACH ROW {}",
        timing_name(meta.timing),
        event_name(meta.event),
        reconstruct_body(&meta.body)
    )
}

fn reconstruct_body(body: &[String]) -> String {
    if body.len() == 1 {
        body[0].clone()
    } else {
        format!("BEGIN {} END", body.join("; "))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::{ColumnDef, TableMeta, DEFAULT_SCHEMA};

    fn session_with_trigger(name: &str, table: &str) -> Session {
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
        session
    }

    #[test]
    fn show_create_trigger_columns_reconstructed_ddl_and_unknown() {
        let session = session_with_trigger("tr_src", "src");

        match show_create_trigger(&session, None, "tr_src") {
            Ok(QueryResult::Rows { columns, rows }) => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "tr_src");
                assert_eq!(rows[0][1], "");
                assert_eq!(
                    rows[0][2],
                    "CREATE TRIGGER `tr_src` BEFORE INSERT ON `src` FOR EACH ROW SET NEW.id = NEW.id"
                );
                assert_eq!(rows[0][3], DEFAULT_CHARSET);
                assert_eq!(rows[0][4], DEFAULT_COLLATION);
                assert_eq!(rows[0][5], DEFAULT_COLLATION);
                assert_eq!(rows[0][6], "");
            }
            other => panic!("expected rows, got {other:?}"),
        }

        match show_create_trigger(&session, None, "no_such_trigger") {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, ER_TRG_DOES_NOT_EXIST);
                assert!(message.contains("no_such_trigger"));
            }
            other => panic!("expected errno 1360, got {other:?}"),
        }
    }
}

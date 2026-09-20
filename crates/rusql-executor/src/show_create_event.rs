//! Documented `SHOW CREATE EVENT` stubs (M100 / M102).
//!
//! `Create Event` is reconstructed from catalog `EventMeta` when present.
//! Unknown names remain MySQL `ER_EVENT_DOES_NOT_EXIST`. Other cells are stubs
//! — not live DEFINER / sql_mode / charset catalogs.

use crate::info_schema::{DEFAULT_CHARSET, DEFAULT_COLLATION};
use crate::{ExecError, QueryResult};
use rusql_core::{EventMeta, Session};

pub(crate) const CREATE_EVENT_VIRTUAL_TABLE: &str = rusql_sql::CREATE_EVENT_VIRTUAL_TABLE;

/// MySQL 8.0 `SHOW CREATE EVENT` column order.
pub(crate) const COLUMNS: [&str; 7] = [
    "Event",
    "sql_mode",
    "time_zone",
    "Create Event",
    "character_set_client",
    "collation_connection",
    "Database Collation",
];

const STUB_TIME_ZONE: &str = "SYSTEM";

/// MySQL `ER_EVENT_DOES_NOT_EXIST`.
const ER_EVENT_DOES_NOT_EXIST: u16 = 1539;

/// `SHOW CREATE EVENT [db.]name` reconstructed from the session catalog.
pub(crate) fn show_create_event(
    session: &Session,
    database: Option<&str>,
    name: &str,
) -> Result<QueryResult, ExecError> {
    let db = database.unwrap_or(session.database.as_str());
    let meta = find_event(session, db, name).ok_or_else(|| ExecError::Mysql {
        code: ER_EVENT_DOES_NOT_EXIST,
        message: rusql_i18n::messages::event_not_found(name),
    })?;
    Ok(QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows: vec![event_row(meta)],
    })
}

fn find_event<'a>(session: &'a Session, schema: &str, name: &str) -> Option<&'a EventMeta> {
    session.catalog.get_event(schema, name).or_else(|| {
        session
            .catalog
            .iter_events()
            .find(|e| e.schema.eq_ignore_ascii_case(schema) && e.name.eq_ignore_ascii_case(name))
    })
}

fn event_row(meta: &EventMeta) -> Vec<String> {
    vec![
        meta.name.clone(),
        String::new(),
        STUB_TIME_ZONE.to_string(),
        create_event_ddl(meta),
        DEFAULT_CHARSET.to_string(),
        DEFAULT_COLLATION.to_string(),
        DEFAULT_COLLATION.to_string(),
    ]
}

fn create_event_ddl(meta: &EventMeta) -> String {
    let name = meta.name.replace('`', "``");
    let schedule = if meta.schedule_type.eq_ignore_ascii_case("RECURRING") {
        let mut s = format!(
            "EVERY {} {}",
            meta.interval_value.as_deref().unwrap_or("1"),
            meta.interval_field.as_deref().unwrap_or("HOUR")
        );
        if let Some(starts) = meta.starts.as_deref() {
            s.push_str(&format!(" STARTS '{starts}'"));
        }
        if let Some(ends) = meta.ends.as_deref() {
            s.push_str(&format!(" ENDS '{ends}'"));
        }
        s
    } else {
        format!("AT '{}'", meta.execute_at.as_deref().unwrap_or(""))
    };
    format!(
        "CREATE EVENT `{name}` ON SCHEDULE {schedule} DO {}",
        meta.body
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::DEFAULT_SCHEMA;

    fn session_with_event() -> Session {
        let mut session = Session::new(1, "root");
        session.catalog.create_event(EventMeta {
            schema: DEFAULT_SCHEMA.into(),
            name: "e".into(),
            schedule_type: "ONE TIME".into(),
            execute_at: Some("2038-01-01 00:00:00".into()),
            interval_value: None,
            interval_field: None,
            status: "ENABLED".into(),
            body: "SELECT 1".into(),
            last_executed: None,
            starts: None,
            ends: None,
        });
        session
    }

    #[test]
    fn show_create_event_unknown_is_errno_1539() {
        let session = Session::new(1, "root");
        match show_create_event(&session, None, "e") {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, ER_EVENT_DOES_NOT_EXIST);
                assert!(message.contains("e"));
            }
            other => panic!("expected errno 1539, got {other:?}"),
        }
        match show_create_event(&session, Some("rusql"), "no_such") {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, ER_EVENT_DOES_NOT_EXIST);
                assert!(message.contains("no_such"));
            }
            other => panic!("expected errno 1539, got {other:?}"),
        }
    }

    #[test]
    fn show_create_event_reconstructed_ddl() {
        let session = session_with_event();
        match show_create_event(&session, None, "e") {
            Ok(QueryResult::Rows { columns, rows }) => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows[0][0], "e");
                assert_eq!(rows[0][2], STUB_TIME_ZONE);
                assert_eq!(
                    rows[0][3],
                    "CREATE EVENT `e` ON SCHEDULE AT '2038-01-01 00:00:00' DO SELECT 1"
                );
            }
            other => panic!("expected reconstructed DDL, got {other:?}"),
        }
    }

    #[test]
    fn show_create_event_reconstructs_starts_ends() {
        let mut session = Session::new(1, "root");
        session.catalog.create_event(EventMeta {
            schema: DEFAULT_SCHEMA.into(),
            name: "r".into(),
            schedule_type: "RECURRING".into(),
            execute_at: None,
            interval_value: Some("1".into()),
            interval_field: Some("HOUR".into()),
            status: "ENABLED".into(),
            body: "SELECT 1".into(),
            last_executed: None,
            starts: Some("2026-09-19 12:00:00".into()),
            ends: Some("2026-09-20 12:00:00".into()),
        });
        match show_create_event(&session, None, "r") {
            Ok(QueryResult::Rows { rows, .. }) => {
                assert_eq!(
                    rows[0][3],
                    "CREATE EVENT `r` ON SCHEDULE EVERY 1 HOUR STARTS '2026-09-19 12:00:00' ENDS '2026-09-20 12:00:00' DO SELECT 1"
                );
            }
            other => panic!("expected STARTS/ENDS in DDL, got {other:?}"),
        }
    }
}

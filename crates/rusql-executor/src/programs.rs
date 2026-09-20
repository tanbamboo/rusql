//! Execute stored programs and triggers (MVP).
use crate::{execute, ExecError, QueryResult};
use rusql_core::{PrivilegeStore, ProgramStore, Session, TableMeta, TriggerEvent, TriggerTiming};
use rusql_sql::{parse_for_session, StoredProgramStmt};
use rusql_storage::{Row, StorageEngine};

pub fn apply_before_insert_triggers(
    session: &Session,
    _table: &str,
    meta: &TableMeta,
    row: &mut Row,
) -> Result<(), ExecError> {
    apply_before_insert_triggers_inner(session, meta, row)
}

struct RowTriggerCtx<'a> {
    meta: &'a TableMeta,
    old_row: &'a Row,
    new_row: Option<&'a Row>,
}

pub fn apply_after_update_triggers<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    meta: &TableMeta,
    old_row: &Row,
    new_row: &Row,
    privileges: Option<&PrivilegeStore>,
) -> Result<(), ExecError> {
    apply_after_triggers(
        engine,
        session,
        RowTriggerCtx {
            meta,
            old_row,
            new_row: Some(new_row),
        },
        TriggerEvent::Update,
        privileges,
    )
}

pub fn apply_after_delete_triggers<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    meta: &TableMeta,
    old_row: &Row,
    privileges: Option<&PrivilegeStore>,
) -> Result<(), ExecError> {
    apply_after_triggers(
        engine,
        session,
        RowTriggerCtx {
            meta,
            old_row,
            new_row: None,
        },
        TriggerEvent::Delete,
        privileges,
    )
}

fn apply_before_insert_triggers_inner(
    session: &Session,
    meta: &TableMeta,
    row: &mut Row,
) -> Result<(), ExecError> {
    let triggers = session.catalog.triggers_for_table(
        &meta.schema,
        &meta.name,
        TriggerTiming::Before,
        TriggerEvent::Insert,
    );
    for trigger in triggers {
        for stmt in &trigger.body {
            let upper = stmt.to_ascii_uppercase();
            if upper.starts_with("SET NEW.") {
                apply_set_new(stmt, meta, row)?;
            } else {
                return Err(ExecError::Message(
                    rusql_i18n::messages::unsupported_program_body(stmt),
                ));
            }
        }
    }
    Ok(())
}

fn apply_after_triggers<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    ctx: RowTriggerCtx<'_>,
    event: TriggerEvent,
    privileges: Option<&PrivilegeStore>,
) -> Result<(), ExecError> {
    let triggers: Vec<_> = session
        .catalog
        .triggers_for_table(
            &ctx.meta.schema,
            &ctx.meta.name,
            TriggerTiming::After,
            event,
        )
        .into_iter()
        .cloned()
        .collect();
    for trigger in triggers {
        for stmt in trigger.body {
            let sql = substitute_old_new(&stmt, ctx.meta, ctx.old_row, ctx.new_row)?;
            let stmts = parse_for_session(&sql, &session.user, &session.host)
                .map_err(|e| ExecError::Message(e.to_string()))?;
            let plans = rusql_planner::plan(session, stmts);
            execute(engine, session, &plans, privileges)?;
        }
    }
    Ok(())
}

fn substitute_old_new(
    stmt: &str,
    meta: &TableMeta,
    old_row: &Row,
    new_row: Option<&Row>,
) -> Result<String, ExecError> {
    let mut out = stmt.to_string();
    for (idx, col) in meta.columns.iter().enumerate() {
        let old_val = old_row.get(idx).map(String::as_str).unwrap_or("");
        replace_row_ref(&mut out, "OLD", &col.name, &quote_sql_string(old_val));
        if let Some(nr) = new_row {
            let new_val = nr.get(idx).map(String::as_str).unwrap_or("");
            replace_row_ref(&mut out, "NEW", &col.name, &quote_sql_string(new_val));
        }
    }
    Ok(out)
}

fn replace_row_ref(out: &mut String, prefix: &str, column: &str, replacement: &str) {
    let pattern = format!("{prefix}.{column}");
    let mut i = 0;
    while i + pattern.len() <= out.len() {
        if out[i..i + pattern.len()].eq_ignore_ascii_case(&pattern) {
            out.replace_range(i..i + pattern.len(), replacement);
            i += replacement.len();
        } else {
            i += 1;
        }
    }
}

fn quote_sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn apply_set_new(stmt: &str, meta: &TableMeta, row: &mut Row) -> Result<(), ExecError> {
    let rest = stmt.get(4..).unwrap_or("").trim();
    let parts: Vec<&str> = rest.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(ExecError::Message(
            rusql_i18n::messages::unsupported_program_body(stmt),
        ));
    }
    let col = parts[0]
        .trim()
        .strip_prefix("NEW.")
        .ok_or_else(|| ExecError::Message(rusql_i18n::messages::unsupported_program_body(stmt)))?;
    let value = strip_quotes(parts[1].trim());
    let idx = meta
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(col))
        .ok_or_else(|| ExecError::Message(format!("column '{col}' not found")))?;
    if idx >= row.len() {
        return Err(ExecError::Message("column index out of range".into()));
    }
    row[idx] = value;
    Ok(())
}

fn strip_quotes(value: &str) -> String {
    if (value.starts_with('\'') && value.ends_with('\''))
        || (value.starts_with('"') && value.ends_with('"'))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

pub fn execute_stored_program<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    store: &mut ProgramStore,
    stmt: StoredProgramStmt,
    privileges: Option<&PrivilegeStore>,
) -> Result<QueryResult, ExecError> {
    match stmt {
        StoredProgramStmt::CreateProcedure(meta) => {
            store
                .create_procedure(meta.clone())
                .map_err(ExecError::Message)?;
            session.catalog.create_procedure(meta);
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        StoredProgramStmt::CreateFunction(meta) => {
            store
                .create_function(meta.clone())
                .map_err(ExecError::Message)?;
            session.catalog.create_function(meta);
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        StoredProgramStmt::DropProcedure {
            schema,
            name,
            if_exists,
        } => match store.drop_procedure(&schema, &name) {
            Ok(()) => {
                session.catalog.drop_procedure(&schema, &name);
                Ok(QueryResult::Ok { rows_affected: 0 })
            }
            Err(_) if if_exists => Ok(QueryResult::Ok { rows_affected: 0 }),
            Err(e) => Err(ExecError::Message(e)),
        },
        StoredProgramStmt::DropFunction {
            schema,
            name,
            if_exists,
        } => match store.drop_function(&schema, &name) {
            Ok(()) => {
                session.catalog.drop_function(&schema, &name);
                Ok(QueryResult::Ok { rows_affected: 0 })
            }
            Err(_) if if_exists => Ok(QueryResult::Ok { rows_affected: 0 }),
            Err(e) => Err(ExecError::Message(e)),
        },
        StoredProgramStmt::CreateTrigger(meta) => {
            store
                .create_trigger(meta.clone())
                .map_err(ExecError::Message)?;
            session.catalog.create_trigger(meta);
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        StoredProgramStmt::DropTrigger {
            schema,
            name,
            if_exists,
        } => match store.drop_trigger_by_name(&schema, &name) {
            Ok(()) => {
                session.catalog.drop_trigger(&schema, &name);
                Ok(QueryResult::Ok { rows_affected: 0 })
            }
            Err(_) if if_exists => Ok(QueryResult::Ok { rows_affected: 0 }),
            Err(e) => Err(ExecError::Message(e)),
        },
        StoredProgramStmt::CreateEvent {
            meta,
            if_not_exists,
        } => match store.create_event(meta.clone()) {
            Ok(()) => {
                session.catalog.create_event(meta);
                Ok(QueryResult::Ok { rows_affected: 0 })
            }
            Err(_) if if_not_exists => Ok(QueryResult::Ok { rows_affected: 0 }),
            Err(_) => Err(ExecError::Mysql {
                code: 1537,
                message: rusql_i18n::messages::event_exists(&meta.name),
            }),
        },
        StoredProgramStmt::DropEvent {
            schema,
            name,
            if_exists,
        } => match store.drop_event(&schema, &name) {
            Ok(()) => {
                session.catalog.drop_event(&schema, &name);
                Ok(QueryResult::Ok { rows_affected: 0 })
            }
            Err(_) if if_exists => Ok(QueryResult::Ok { rows_affected: 0 }),
            Err(_) => Err(ExecError::Mysql {
                code: 1539,
                message: rusql_i18n::messages::event_not_found(&name),
            }),
        },
        StoredProgramStmt::AlterEvent {
            schema,
            name,
            schedule_type,
            execute_at,
            interval_value,
            interval_field,
            status,
            rename_schema,
            rename_name,
            body,
            starts,
            ends,
        } => {
            let mut meta =
                store
                    .get_event(&schema, &name)
                    .cloned()
                    .ok_or_else(|| ExecError::Mysql {
                        code: 1539,
                        message: rusql_i18n::messages::event_not_found(&name),
                    })?;
            if let Some(schedule_type) = schedule_type {
                meta.schedule_type = schedule_type;
                meta.execute_at = execute_at;
                meta.interval_value = interval_value;
                meta.interval_field = interval_field;
            }
            if let Some(status) = status {
                meta.status = status;
            }
            if let Some(body) = body {
                meta.body = body;
            }
            if let Some(starts) = starts {
                meta.starts = Some(starts);
            }
            if let Some(ends) = ends {
                meta.ends = Some(ends);
            }
            let old_schema = schema;
            let old_name = name;
            if let (Some(new_schema), Some(new_name)) = (rename_schema, rename_name) {
                let same = new_schema.eq_ignore_ascii_case(&old_schema)
                    && new_name.eq_ignore_ascii_case(&old_name);
                if !same && store.get_event(&new_schema, &new_name).is_some() {
                    return Err(ExecError::Mysql {
                        code: 1537,
                        message: rusql_i18n::messages::event_exists(&new_name),
                    });
                }
                store
                    .drop_event(&old_schema, &old_name)
                    .map_err(ExecError::Message)?;
                session.catalog.drop_event(&old_schema, &old_name);
                meta.schema = new_schema;
                meta.name = new_name;
            }
            store.put_event(meta.clone());
            session.catalog.create_event(meta);
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        StoredProgramStmt::Call { schema, name } => {
            let proc = store
                .get_procedure(&schema, &name)
                .ok_or_else(|| {
                    ExecError::Message(rusql_i18n::messages::procedure_not_found(&name))
                })?
                .clone();
            let mut last = QueryResult::Ok { rows_affected: 0 };
            for sql in &proc.body {
                let stmts = parse_for_session(sql, &session.user, &session.host)
                    .map_err(|e| ExecError::Message(e.to_string()))?;
                let plans = rusql_planner::plan(session, stmts);
                for r in execute(engine, session, &plans, privileges)? {
                    last = r;
                }
            }
            Ok(last)
        }
    }
}

/// UTC `YYYY-MM-DD HH:MM:SS` used to decide whether an `AT` event is due (M104).
pub fn utc_now_stamp() -> String {
    crate::expr::now_string()
}

fn event_is_due(meta: &rusql_core::EventMeta, now: &str) -> bool {
    if !meta.status.eq_ignore_ascii_case("ENABLED") {
        return false;
    }
    if meta.schedule_type.eq_ignore_ascii_case("ONE TIME") {
        let Some(at) = meta.execute_at.as_deref() else {
            return false;
        };
        return at <= now;
    }
    if !meta.schedule_type.eq_ignore_ascii_case("RECURRING") {
        return false;
    }
    if meta.starts.as_deref().is_some_and(|starts| now < starts) {
        return false;
    }
    if meta.ends.as_deref().is_some_and(|ends| now > ends) {
        return false;
    }
    let Some(value) = meta.interval_value.as_deref() else {
        return false;
    };
    let Some(field) = meta.interval_field.as_deref() else {
        return false;
    };
    match meta.last_executed.as_deref() {
        None => true,
        Some(last) => crate::expr::add_schedule_interval(last, value, field)
            .is_some_and(|next| next.as_str() <= now),
    }
}

fn execute_event_body<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    sql: &str,
    privileges: Option<&PrivilegeStore>,
) -> Result<(), ExecError> {
    let stmts = parse_for_session(sql, &session.user, &session.host)
        .map_err(|e| ExecError::Message(e.to_string()))?;
    let plans = rusql_planner::plan(session, stmts);
    execute(engine, session, &plans, privileges)?;
    Ok(())
}

/// Run due ENABLED events: one-time `AT` (then drop) and recurring `EVERY` (keep + watermark).
///
/// `DO` errors leave the catalog row in place and do not fail the caller.
pub fn run_due_events<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    store: &mut ProgramStore,
    privileges: Option<&PrivilegeStore>,
    now: &str,
) -> Result<(), ExecError> {
    let due: Vec<_> = store
        .events
        .values()
        .filter(|meta| event_is_due(meta, now))
        .cloned()
        .collect();
    for mut meta in due {
        match execute_event_body(engine, session, &meta.body, privileges) {
            Ok(()) => {
                if meta.schedule_type.eq_ignore_ascii_case("ONE TIME") {
                    let _ = store.drop_event(&meta.schema, &meta.name);
                    session.catalog.drop_event(&meta.schema, &meta.name);
                } else {
                    meta.last_executed = Some(now.to_string());
                    store.put_event(meta.clone());
                    session.catalog.create_event(meta);
                }
            }
            Err(_) => {
                // Keep the row so a later COM_QUERY can retry; do not fail the client.
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::{ColumnDef, TriggerMeta, DEFAULT_SCHEMA};
    use rusql_storage::HeapEngine;

    #[test]
    fn before_insert_trigger_sets_column() {
        let mut session = Session::new(1, "root");
        let meta = TableMeta {
            name: "t".into(),
            schema: DEFAULT_SCHEMA.into(),
            columns: vec![
                ColumnDef::new("id", "INT"),
                ColumnDef::new("status", "VARCHAR(16)"),
            ],
            auto_increment_next: None,
            ..Default::default()
        };
        session.catalog.create_table(meta.clone());
        session.catalog.create_trigger(TriggerMeta {
            schema: DEFAULT_SCHEMA.into(),
            table: "t".into(),
            name: "tr".into(),
            timing: TriggerTiming::Before,
            event: TriggerEvent::Insert,
            body: vec!["SET NEW.status = 'active'".into()],
        });
        let mut row = vec!["1".into(), "".into()];
        apply_before_insert_triggers(&session, "t", &meta, &mut row).unwrap();
        assert_eq!(row[1], "active");
    }

    #[test]
    fn call_procedure_inserts_row() {
        use rusql_sql::try_parse_stored_program;
        let mut engine = HeapEngine::new();
        let mut session = Session::new(1, "root");
        let mut store = ProgramStore::default();
        engine
            .create_table(TableMeta {
                name: "t".into(),
                schema: DEFAULT_SCHEMA.into(),
                columns: vec![ColumnDef::new("id", "INT")],
                auto_increment_next: None,
                ..Default::default()
            })
            .unwrap();
        session.catalog.create_table(TableMeta {
            name: "t".into(),
            schema: DEFAULT_SCHEMA.into(),
            columns: vec![ColumnDef::new("id", "INT")],
            auto_increment_next: None,
            ..Default::default()
        });
        let create =
            try_parse_stored_program("CREATE PROCEDURE p() BEGIN INSERT INTO t VALUES (42); END")
                .unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, create, None).unwrap();
        let call = try_parse_stored_program("CALL p()").unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, call, None).unwrap();
        assert_eq!(engine.scan("t").unwrap(), vec![vec!["42".to_string()]]);
    }

    #[test]
    fn after_update_trigger_inserts_audit_row() {
        use rusql_sql::{parse, try_parse_stored_program};
        let mut engine = HeapEngine::new();
        let mut session = Session::new(1, "root");
        let mut store = ProgramStore::default();
        for sql in [
            "CREATE TABLE src (id INT, name VARCHAR(16))",
            "CREATE TABLE audit (id INT, name VARCHAR(16))",
            "INSERT INTO src VALUES (1, 'a')",
        ] {
            let stmts = parse(sql).unwrap();
            let plans = rusql_planner::plan(&session, stmts);
            execute(&mut engine, &mut session, &plans, None).unwrap();
        }
        let create = try_parse_stored_program(
            "CREATE TRIGGER tr AFTER UPDATE ON src FOR EACH ROW INSERT INTO audit VALUES (OLD.id, NEW.name)",
        )
        .unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, create, None).unwrap();
        let update = parse("UPDATE src SET name = 'b' WHERE id = 1").unwrap();
        let plans = rusql_planner::plan(&session, update);
        execute(&mut engine, &mut session, &plans, None).unwrap();
        assert_eq!(
            engine.scan("audit").unwrap(),
            vec![vec!["1".to_string(), "b".to_string()]]
        );
    }

    #[test]
    fn after_delete_trigger_inserts_audit_row() {
        use rusql_sql::{parse, try_parse_stored_program};
        let mut engine = HeapEngine::new();
        let mut session = Session::new(1, "root");
        let mut store = ProgramStore::default();
        for sql in [
            "CREATE TABLE src (id INT, name VARCHAR(16))",
            "CREATE TABLE audit (id INT, action VARCHAR(16))",
            "INSERT INTO src VALUES (1, 'a')",
        ] {
            let stmts = parse(sql).unwrap();
            let plans = rusql_planner::plan(&session, stmts);
            execute(&mut engine, &mut session, &plans, None).unwrap();
        }
        let create = try_parse_stored_program(
            "CREATE TRIGGER tr AFTER DELETE ON src FOR EACH ROW INSERT INTO audit VALUES (OLD.id, 'deleted')",
        )
        .unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, create, None).unwrap();
        let delete = parse("DELETE FROM src WHERE id = 1").unwrap();
        let plans = rusql_planner::plan(&session, delete);
        execute(&mut engine, &mut session, &plans, None).unwrap();
        assert_eq!(
            engine.scan("audit").unwrap(),
            vec![vec!["1".to_string(), "deleted".to_string()]]
        );
    }

    #[test]
    fn create_function_scalar_in_select() {
        use rusql_sql::{parse, try_parse_stored_program};
        let mut engine = HeapEngine::new();
        let mut session = Session::new(1, "root");
        let mut store = ProgramStore::default();
        let create = try_parse_stored_program(
            "CREATE FUNCTION forty_two() RETURNS INT BEGIN RETURN 42; END",
        )
        .unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, create, None).unwrap();
        let q = parse("SELECT forty_two()").unwrap();
        let plans = rusql_planner::plan(&session, q);
        let results = execute(&mut engine, &mut session, &plans, None).unwrap();
        let QueryResult::Rows { rows, .. } = &results[0] else {
            panic!("expected rows");
        };
        assert_eq!(rows[0][0], "42");
    }

    #[test]
    fn create_function_used_in_expression() {
        use rusql_sql::{parse, try_parse_stored_program};
        let mut engine = HeapEngine::new();
        let mut session = Session::new(1, "root");
        let mut store = ProgramStore::default();
        let create =
            try_parse_stored_program("CREATE FUNCTION one() RETURNS INT BEGIN RETURN 1; END")
                .unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, create, None).unwrap();
        let q = parse("SELECT one() + 1").unwrap();
        let plans = rusql_planner::plan(&session, q);
        let results = execute(&mut engine, &mut session, &plans, None).unwrap();
        let QueryResult::Rows { rows, .. } = &results[0] else {
            panic!("expected rows");
        };
        assert_eq!(rows[0][0], "2");
    }

    #[test]
    fn create_event_catalog_show_and_drop() {
        use rusql_sql::try_parse_stored_program;
        let mut engine = HeapEngine::new();
        let mut session = Session::new(1, "root");
        let mut store = ProgramStore::default();
        let create = try_parse_stored_program(
            "CREATE EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' DO SELECT 1",
        )
        .unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, create, None).unwrap();
        assert!(session.catalog.get_event("rusql", "e").is_some());

        let dup = try_parse_stored_program("CREATE EVENT e ON SCHEDULE EVERY 1 HOUR DO SELECT 1")
            .unwrap();
        match execute_stored_program(&mut engine, &mut session, &mut store, dup, None) {
            Err(ExecError::Mysql { code, .. }) => assert_eq!(code, 1537),
            other => panic!("expected errno 1537, got {other:?}"),
        }

        let if_not = try_parse_stored_program(
            "CREATE EVENT IF NOT EXISTS e ON SCHEDULE EVERY 1 HOUR DO SELECT 1",
        )
        .unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, if_not, None).unwrap();

        let drop = try_parse_stored_program("DROP EVENT e").unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, drop, None).unwrap();
        assert!(session.catalog.get_event("rusql", "e").is_none());

        let missing = try_parse_stored_program("DROP EVENT e").unwrap();
        match execute_stored_program(&mut engine, &mut session, &mut store, missing, None) {
            Err(ExecError::Mysql { code, .. }) => assert_eq!(code, 1539),
            other => panic!("expected errno 1539, got {other:?}"),
        }
        let if_exists = try_parse_stored_program("DROP EVENT IF EXISTS e").unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, if_exists, None).unwrap();
    }

    #[test]
    fn alter_event_updates_catalog() {
        use rusql_sql::try_parse_stored_program;
        let mut engine = HeapEngine::new();
        let mut session = Session::new(1, "root");
        let mut store = ProgramStore::default();
        let create = try_parse_stored_program(
            "CREATE EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' DO SELECT 1",
        )
        .unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, create, None).unwrap();

        let alter = try_parse_stored_program("ALTER EVENT e ON SCHEDULE EVERY 1 DAY").unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, alter, None).unwrap();
        let meta = session.catalog.get_event("rusql", "e").unwrap();
        assert_eq!(meta.schedule_type, "RECURRING");
        assert_eq!(meta.interval_field.as_deref(), Some("DAY"));
        assert!(meta.execute_at.is_none());

        let disable = try_parse_stored_program("ALTER EVENT e DISABLE").unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, disable, None).unwrap();
        assert_eq!(
            session.catalog.get_event("rusql", "e").unwrap().status,
            "DISABLED"
        );

        let rename = try_parse_stored_program("ALTER EVENT e RENAME TO e2 DO SELECT 2").unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, rename, None).unwrap();
        assert!(session.catalog.get_event("rusql", "e").is_none());
        let renamed = session.catalog.get_event("rusql", "e2").unwrap();
        assert_eq!(renamed.body, "SELECT 2");
        assert_eq!(renamed.status, "DISABLED");

        let window = try_parse_stored_program(
            "ALTER EVENT e2 STARTS '2026-01-01 00:00:00' ENDS '2026-12-31 00:00:00'",
        )
        .unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, window, None).unwrap();
        let windowed = session.catalog.get_event("rusql", "e2").unwrap();
        assert_eq!(windowed.starts.as_deref(), Some("2026-01-01 00:00:00"));
        assert_eq!(windowed.ends.as_deref(), Some("2026-12-31 00:00:00"));

        let missing = try_parse_stored_program("ALTER EVENT e ENABLE").unwrap();
        match execute_stored_program(&mut engine, &mut session, &mut store, missing, None) {
            Err(ExecError::Mysql { code, .. }) => assert_eq!(code, 1539),
            other => panic!("expected errno 1539, got {other:?}"),
        }
    }

    #[test]
    fn event_scheduler_runs_due_at_and_skips_neighbors() {
        use rusql_sql::try_parse_stored_program;
        let mut engine = HeapEngine::new();
        let mut session = Session::new(1, "root");
        let mut store = ProgramStore::default();
        engine
            .create_table(TableMeta {
                name: "t".into(),
                schema: DEFAULT_SCHEMA.into(),
                columns: vec![ColumnDef::new("id", "INT")],
                auto_increment_next: None,
                ..Default::default()
            })
            .unwrap();
        session.catalog.create_table(TableMeta {
            name: "t".into(),
            schema: DEFAULT_SCHEMA.into(),
            columns: vec![ColumnDef::new("id", "INT")],
            auto_increment_next: None,
            ..Default::default()
        });
        for sql in [
            "CREATE EVENT due_e ON SCHEDULE AT '2000-01-01 00:00:00' DO INSERT INTO t VALUES (1)",
            "CREATE EVENT future_e ON SCHEDULE AT '2038-01-01 00:00:00' DO INSERT INTO t VALUES (2)",
            "CREATE EVENT rec_e ON SCHEDULE EVERY 1 HOUR DO INSERT INTO t VALUES (3)",
            "CREATE EVENT rec_off ON SCHEDULE EVERY 1 MINUTE DISABLE DO INSERT INTO t VALUES (5)",
            "CREATE EVENT off_e ON SCHEDULE AT '2000-01-01 00:00:00' DISABLE DO INSERT INTO t VALUES (4)",
            "CREATE EVENT bad_e ON SCHEDULE AT '2000-01-01 00:00:00' DO INSERT INTO missing VALUES (9)",
        ] {
            let stmt = try_parse_stored_program(sql).unwrap();
            execute_stored_program(&mut engine, &mut session, &mut store, stmt, None).unwrap();
        }

        run_due_events(
            &mut engine,
            &mut session,
            &mut store,
            None,
            "2026-09-19 12:00:00",
        )
        .unwrap();
        let mut rows = engine.scan("t").unwrap();
        rows.sort();
        assert_eq!(rows, vec![vec!["1".to_string()], vec!["3".to_string()]]);
        assert!(store.get_event("rusql", "due_e").is_none());
        assert!(session.catalog.get_event("rusql", "due_e").is_none());
        assert!(store.get_event("rusql", "future_e").is_some());
        let rec = store.get_event("rusql", "rec_e").unwrap();
        assert_eq!(rec.last_executed.as_deref(), Some("2026-09-19 12:00:00"));
        assert!(store.get_event("rusql", "off_e").is_some());
        assert!(store.get_event("rusql", "bad_e").is_some());
        assert!(store
            .get_event("rusql", "rec_off")
            .unwrap()
            .last_executed
            .is_none());

        run_due_events(
            &mut engine,
            &mut session,
            &mut store,
            None,
            "2026-09-19 12:00:00",
        )
        .unwrap();
        let mut rows = engine.scan("t").unwrap();
        rows.sort();
        assert_eq!(
            rows,
            vec![vec!["1".to_string()], vec!["3".to_string()]],
            "EVERY 1 HOUR must not re-fire at the same timestamp"
        );

        run_due_events(
            &mut engine,
            &mut session,
            &mut store,
            None,
            "2026-09-19 13:00:00",
        )
        .unwrap();
        let mut rows = engine.scan("t").unwrap();
        rows.sort();
        assert_eq!(
            rows,
            vec![
                vec!["1".to_string()],
                vec!["3".to_string()],
                vec!["3".to_string()]
            ]
        );
        assert_eq!(
            store
                .get_event("rusql", "rec_e")
                .unwrap()
                .last_executed
                .as_deref(),
            Some("2026-09-19 13:00:00")
        );

        let alter = try_parse_stored_program("ALTER EVENT future_e DISABLE").unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, alter, None).unwrap();
        assert_eq!(
            session
                .catalog
                .get_event("rusql", "future_e")
                .unwrap()
                .status,
            "DISABLED"
        );
    }

    #[test]
    fn event_scheduler_starts_ends_gates() {
        use rusql_sql::try_parse_stored_program;
        let mut engine = HeapEngine::new();
        let mut session = Session::new(1, "root");
        let mut store = ProgramStore::default();
        engine
            .create_table(TableMeta {
                name: "t".into(),
                schema: DEFAULT_SCHEMA.into(),
                columns: vec![ColumnDef::new("id", "INT")],
                auto_increment_next: None,
                ..Default::default()
            })
            .unwrap();
        session.catalog.create_table(TableMeta {
            name: "t".into(),
            schema: DEFAULT_SCHEMA.into(),
            columns: vec![ColumnDef::new("id", "INT")],
            auto_increment_next: None,
            ..Default::default()
        });
        for sql in [
            "CREATE EVENT before_s ON SCHEDULE EVERY 1 HOUR STARTS '2026-09-19 13:00:00' DO INSERT INTO t VALUES (1)",
            "CREATE EVENT after_e ON SCHEDULE EVERY 1 HOUR STARTS '2000-01-01 00:00:00' ENDS '2026-09-19 11:00:00' DO INSERT INTO t VALUES (2)",
            "CREATE EVENT in_win ON SCHEDULE EVERY 1 HOUR STARTS '2026-09-19 12:00:00' ENDS '2026-09-19 12:00:00' DO INSERT INTO t VALUES (3)",
            "CREATE EVENT due_at ON SCHEDULE AT '2000-01-01 00:00:00' DO INSERT INTO t VALUES (4)",
        ] {
            let stmt = try_parse_stored_program(sql).unwrap();
            execute_stored_program(&mut engine, &mut session, &mut store, stmt, None).unwrap();
        }

        run_due_events(
            &mut engine,
            &mut session,
            &mut store,
            None,
            "2026-09-19 12:00:00",
        )
        .unwrap();
        let mut rows = engine.scan("t").unwrap();
        rows.sort();
        assert_eq!(
            rows,
            vec![vec!["3".to_string()], vec!["4".to_string()]],
            "STARTS/ENDS gates EVERY; inclusive window fires; due AT still drops"
        );
        assert!(store
            .get_event("rusql", "before_s")
            .unwrap()
            .last_executed
            .is_none());
        assert!(store
            .get_event("rusql", "after_e")
            .unwrap()
            .last_executed
            .is_none());
        assert_eq!(
            store
                .get_event("rusql", "in_win")
                .unwrap()
                .last_executed
                .as_deref(),
            Some("2026-09-19 12:00:00")
        );
        assert!(store.get_event("rusql", "due_at").is_none());

        run_due_events(
            &mut engine,
            &mut session,
            &mut store,
            None,
            "2026-09-19 12:00:00",
        )
        .unwrap();
        let mut rows = engine.scan("t").unwrap();
        rows.sort();
        assert_eq!(
            rows,
            vec![vec!["3".to_string()], vec!["4".to_string()]],
            "watermark must still suppress a second fire at the same timestamp"
        );

        let alter =
            try_parse_stored_program("ALTER EVENT before_s STARTS '2000-01-01 00:00:00'").unwrap();
        execute_stored_program(&mut engine, &mut session, &mut store, alter, None).unwrap();
        run_due_events(
            &mut engine,
            &mut session,
            &mut store,
            None,
            "2026-09-19 12:00:00",
        )
        .unwrap();
        let mut rows = engine.scan("t").unwrap();
        rows.sort();
        assert_eq!(
            rows,
            vec![
                vec!["1".to_string()],
                vec!["3".to_string()],
                vec!["4".to_string()]
            ]
        );
        assert_eq!(
            store
                .get_event("rusql", "before_s")
                .unwrap()
                .starts
                .as_deref(),
            Some("2000-01-01 00:00:00")
        );
    }
}

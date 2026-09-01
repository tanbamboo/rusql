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
        .triggers_for_table(&ctx.meta.schema, &ctx.meta.name, TriggerTiming::After, event)
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
}

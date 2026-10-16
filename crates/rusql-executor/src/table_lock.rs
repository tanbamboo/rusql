//! Session `LOCK TABLES` enforcement in the executor (M163).

use crate::{resolve_object_storage_key, ExecError, Session};
use rusql_storage::{TableLockMode, TableLockRegistry, TableLockWaitTimeout};
use sqlparser::ast::{FromTable, LockTable, LockTableType, ObjectName, Statement, TableFactor};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

thread_local! {
    static TABLE_LOCK_CTX: RefCell<Option<TableLockContext>> = const { RefCell::new(None) };
}

#[derive(Clone)]
struct TableLockContext {
    registry: Arc<TableLockRegistry>,
    session_id: u64,
    /// `Some` when this connection executed `LOCK TABLES` and has not `UNLOCK TABLES` yet.
    held: Option<HashMap<String, TableLockMode>>,
}

pub fn set_table_lock_context(
    registry: Arc<TableLockRegistry>,
    session_id: u64,
    held: Option<HashMap<String, TableLockMode>>,
) {
    TABLE_LOCK_CTX.with(|cell| {
        *cell.borrow_mut() = Some(TableLockContext {
            registry,
            session_id,
            held,
        });
    });
}

pub fn clear_table_lock_context() {
    TABLE_LOCK_CTX.with(|cell| *cell.borrow_mut() = None);
}

pub fn table_lock_wait_error(_: TableLockWaitTimeout) -> ExecError {
    ExecError::Mysql {
        code: 1205,
        message: rusql_i18n::messages::sql_lock_wait_timeout(),
    }
}

pub fn table_not_locked_error(table: &str) -> ExecError {
    ExecError::Mysql {
        code: 1100,
        message: rusql_i18n::messages::sql_table_not_locked_with_lock_tables(table),
    }
}

pub fn table_read_lock_write_error(table: &str) -> ExecError {
    ExecError::Mysql {
        code: 1099,
        message: rusql_i18n::messages::sql_table_locked_with_read_lock(table),
    }
}

/// Resolve `LOCK TABLES` entries to storage keys and validate tables exist.
pub fn resolve_lock_table_spec(
    session: &Session,
    tables: &[LockTable],
) -> Result<Vec<(String, TableLockMode)>, ExecError> {
    let mut spec = Vec::with_capacity(tables.len());
    for lt in tables {
        let key = lock_table_storage_key(session, &lt.table)?;
        if session.catalog.get_table(&key).is_none() {
            return Err(ExecError::Storage(
                rusql_storage::StorageError::table_not_found(&display_table_name(&key)),
            ));
        }
        let mode = match lt.lock_type {
            LockTableType::Read { .. } => TableLockMode::Read,
            LockTableType::Write { .. } => TableLockMode::Write,
        };
        spec.push((key, mode));
    }
    Ok(spec)
}

fn lock_table_storage_key(
    session: &Session,
    name: &sqlparser::ast::Ident,
) -> Result<String, ExecError> {
    resolve_object_storage_key(session, &ObjectName(vec![name.clone()]))
}

fn display_table_name(storage_key: &str) -> String {
    storage_key.to_string()
}

pub fn acquire_lock_tables(
    registry: &TableLockRegistry,
    session_id: u64,
    spec: &[(String, TableLockMode)],
    timeout: Duration,
) -> Result<(), ExecError> {
    registry
        .lock_tables(session_id, spec, timeout)
        .map_err(table_lock_wait_error)
}

pub fn release_lock_tables(registry: &TableLockRegistry, session_id: u64) {
    registry.unlock_tables(session_id);
}

/// Before DML/SELECT on `table`, enforce lock mode and wait on other sessions' table locks.
pub fn ensure_table_lock_access(table: &str, need_write: bool) -> Result<(), ExecError> {
    let ctx = TABLE_LOCK_CTX.with(|cell| cell.borrow().clone());
    let Some(ctx) = ctx else {
        return Ok(());
    };
    if let Some(ref held) = ctx.held {
        let Some(mode) = held.get(table) else {
            return Err(table_not_locked_error(table));
        };
        if need_write && *mode == TableLockMode::Read {
            return Err(table_read_lock_write_error(table));
        }
    }
    let timeout = Duration::from_secs(3600);
    ctx.registry
        .wait_for_access(ctx.session_id, table, need_write, timeout)
        .map_err(table_lock_wait_error)
}

/// Tables referenced by a statement for lock checks (MVP: base-table DML/SELECT/TRUNCATE).
pub fn statement_table_access(stmt: &Statement) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    match stmt {
        Statement::Insert(insert) => {
            push_object_name(&mut out, &insert.table_name, true);
        }
        Statement::Delete(delete) => {
            let tables = match &delete.from {
                FromTable::WithFromKeyword(t) | FromTable::WithoutKeyword(t) => t,
            };
            for table in tables {
                push_table_with_ref(&mut out, table, true);
            }
        }
        Statement::Update { table, .. } => {
            push_table_with_ref(&mut out, table, true);
        }
        Statement::Query(q) => {
            if let sqlparser::ast::SetExpr::Select(select) = q.body.as_ref() {
                for item in &select.from {
                    push_table_with_ref(&mut out, item, false);
                }
            }
        }
        Statement::Truncate { table_names, .. } => {
            for t in table_names {
                push_object_name(&mut out, &t.name, true);
            }
        }
        _ => {}
    }
    out
}

fn push_object_name(out: &mut Vec<(String, bool)>, name: &ObjectName, write: bool) {
    out.push((object_name_only(name), write));
}

fn object_name_only(name: &ObjectName) -> String {
    name.0
        .iter()
        .map(|i| i.value.clone())
        .collect::<Vec<_>>()
        .join(".")
}

fn push_table_with_ref(
    out: &mut Vec<(String, bool)>,
    table: &sqlparser::ast::TableWithJoins,
    write: bool,
) {
    if let TableFactor::Table { name, .. } = &table.relation {
        push_object_name(out, name, write);
    }
}

/// Map raw table reference to storage key (caller must pass session when executing).
pub fn resolve_statement_tables(
    session: &Session,
    raw: &[(String, bool)],
) -> Result<Vec<(String, bool)>, ExecError> {
    raw.iter()
        .map(|(name, write)| {
            let parts: Vec<_> = name.split('.').collect();
            let object = match parts.as_slice() {
                [table] => ObjectName(vec![sqlparser::ast::Ident::new(*table)]),
                [schema, table] => ObjectName(vec![
                    sqlparser::ast::Ident::new(*schema),
                    sqlparser::ast::Ident::new(*table),
                ]),
                _ => {
                    return Err(ExecError::Message(format!(
                        "invalid table reference '{name}'"
                    )))
                }
            };
            Ok((resolve_object_storage_key(session, &object)?, *write))
        })
        .collect()
}

pub fn wait_statement_table_locks(
    session: &Session,
    stmt: &Statement,
    timeout: Duration,
) -> Result<(), ExecError> {
    let ctx = TABLE_LOCK_CTX.with(|cell| cell.borrow().clone());
    let Some(ctx) = ctx else {
        return Ok(());
    };
    wait_statement_table_locks_inner(
        &ctx.registry,
        ctx.session_id,
        ctx.held.as_ref(),
        session,
        stmt,
        timeout,
    )
}

/// Like [`wait_statement_table_locks`] but explicit registry (for `spawn_blocking` on the server).
pub fn plan_statement_table_lock_waits(
    session: &Session,
    stmt: &Statement,
    held: Option<&HashMap<String, TableLockMode>>,
) -> Result<Vec<(String, bool)>, ExecError> {
    let raw = statement_table_access(stmt);
    let resolved = resolve_statement_tables(session, &raw)?;
    for (table, need_write) in &resolved {
        if let Some(held) = held {
            if !held.contains_key(table) {
                return Err(table_not_locked_error(table));
            }
            if *need_write && held.get(table) == Some(&TableLockMode::Read) {
                return Err(table_read_lock_write_error(table));
            }
        }
    }
    Ok(resolved)
}

pub fn wait_planned_table_locks(
    registry: &TableLockRegistry,
    session_id: u64,
    plan: &[(String, bool)],
    timeout: Duration,
) -> Result<(), ExecError> {
    for (table, need_write) in plan {
        registry
            .wait_for_access(session_id, table, *need_write, timeout)
            .map_err(table_lock_wait_error)?;
    }
    Ok(())
}

pub fn wait_statement_table_locks_blocking(
    registry: &TableLockRegistry,
    session_id: u64,
    held: Option<&HashMap<String, TableLockMode>>,
    session: &Session,
    stmt: &Statement,
    timeout: Duration,
) -> Result<(), ExecError> {
    let plan = plan_statement_table_lock_waits(session, stmt, held)?;
    wait_planned_table_locks(registry, session_id, &plan, timeout)
}

fn wait_statement_table_locks_inner(
    registry: &TableLockRegistry,
    session_id: u64,
    held: Option<&HashMap<String, TableLockMode>>,
    session: &Session,
    stmt: &Statement,
    timeout: Duration,
) -> Result<(), ExecError> {
    let raw = statement_table_access(stmt);
    let resolved = resolve_statement_tables(session, &raw)?;
    for (table, need_write) in resolved {
        if let Some(held) = held {
            if !held.contains_key(&table) {
                return Err(table_not_locked_error(&table));
            }
            if need_write && held.get(&table) == Some(&TableLockMode::Read) {
                return Err(table_read_lock_write_error(&table));
            }
        }
        registry
            .wait_for_access(session_id, &table, need_write, timeout)
            .map_err(table_lock_wait_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::Session;
    use sqlparser::ast::Statement;

    #[test]
    fn insert_statement_table_access_needs_write() {
        let stmts = rusql_sql::parse("INSERT INTO lt_t VALUES (1)").unwrap();
        let raw = statement_table_access(&stmts[0]);
        assert_eq!(raw, vec![("lt_t".to_string(), true)]);
    }

    #[test]
    fn lock_tables_unknown_table_resolves_to_1146() {
        let session = Session::new(1, "root");
        let stmts = rusql_sql::parse("LOCK TABLES missing WRITE").unwrap();
        let Statement::LockTables { tables } = &stmts[0] else {
            panic!("expected LockTables");
        };
        let err = resolve_lock_table_spec(&session, tables).unwrap_err();
        match err {
            ExecError::Storage(_) => {}
            other => panic!("expected storage 1146, got {other:?}"),
        }
    }
}

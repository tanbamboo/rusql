//! Row and gap exclusive locks for `SELECT … FOR UPDATE` and conflicting DML (M157, M161).

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use rusql_core::{Session, TableMeta};
use rusql_planner::Plan;
use rusql_storage::{
    GapLockKey, LockTarget, Row, RowKey, RowLockAcquireError, RowLockRegistry, StorageEngine,
    TxIsolationLevel,
};
use sqlparser::ast::SetExpr;
use sqlparser::ast::{Expr, LockClause, LockType, Statement};

use crate::fk::matching_rows;
use crate::where_filter::{extract_eq_predicate, parse_where_filter, pk_range_from_filter};
use crate::{resolve_object_storage_key, resolve_table_with_joins, ExecError};

thread_local! {
    static ROW_LOCK_REGISTRY: RefCell<Option<Arc<RowLockRegistry>>> = const { RefCell::new(None) };
}

/// Install the process-wide row lock table for the current thread (one COM_QUERY).
pub fn set_row_lock_registry(reg: Arc<RowLockRegistry>) {
    ROW_LOCK_REGISTRY.with(|cell| *cell.borrow_mut() = Some(reg));
}

/// Clear the row lock table after statement execution.
pub fn clear_row_lock_registry() {
    ROW_LOCK_REGISTRY.with(|cell| *cell.borrow_mut() = None);
}

fn registry() -> Option<Arc<RowLockRegistry>> {
    ROW_LOCK_REGISTRY.with(|cell| cell.borrow().clone())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectLockMode {
    None,
    ForUpdate,
    ForShare,
}

pub(crate) fn select_lock_mode(locks: &[LockClause]) -> SelectLockMode {
    match locks.first() {
        Some(lock) => match lock.lock_type {
            LockType::Update => SelectLockMode::ForUpdate,
            LockType::Share => SelectLockMode::ForShare,
        },
        None => SelectLockMode::None,
    }
}

/// Explicit `FOR UPDATE` / `FOR SHARE`, or implicit `FOR UPDATE` under **SERIALIZABLE** (M159).
pub(crate) fn effective_select_lock_mode(
    session: &Session,
    locks: &[LockClause],
) -> SelectLockMode {
    match select_lock_mode(locks) {
        mode @ (SelectLockMode::ForUpdate | SelectLockMode::ForShare) => mode,
        SelectLockMode::None => {
            if crate::session_tx_isolation(session) == TxIsolationLevel::Serializable {
                SelectLockMode::ForUpdate
            } else {
                SelectLockMode::None
            }
        }
    }
}

/// Parse lock clauses; `NOWAIT` / `SKIP LOCKED` / `OF` remain accepted without extra behavior (M157).
pub(crate) fn accept_select_lock_clauses(locks: &[LockClause]) {
    for lock in locks {
        match lock.lock_type {
            LockType::Share | LockType::Update => {}
        }
        let _ = (&lock.of, &lock.nonblock);
    }
}

fn lock_wait_timeout(session: &Session) -> Duration {
    const DEFAULT_SECS: u64 = 50;
    let key = "innodb_lock_wait_timeout";
    if let Some(v) = session.session_vars.get(key) {
        if let Ok(secs) = v.parse::<u64>() {
            return Duration::from_secs(secs);
        }
    }
    Duration::from_secs(DEFAULT_SECS)
}

fn gap_locks_enabled(session: &Session) -> bool {
    matches!(
        crate::session_tx_isolation(session),
        TxIsolationLevel::RepeatableRead | TxIsolationLevel::Serializable
    )
}

fn pk_values_for_row(meta: &TableMeta, row: &Row) -> Option<Vec<String>> {
    let pk_cols: Vec<_> = meta.columns.iter().filter(|c| c.primary_key).collect();
    if pk_cols.len() != 1 {
        return None;
    }
    let pk = &pk_cols[0];
    let idx = meta
        .columns
        .iter()
        .position(|c| c.name == pk.name)
        .unwrap_or(0);
    Some(vec![row.get(idx).cloned().unwrap_or_default()])
}

fn single_pk_column(meta: &TableMeta) -> Option<String> {
    let pk_cols: Vec<_> = meta
        .columns
        .iter()
        .filter(|c| c.primary_key)
        .map(|c| c.name.clone())
        .collect();
    if pk_cols.len() == 1 {
        Some(pk_cols[0].clone())
    } else {
        None
    }
}

fn pk_range_gap_lock(meta: &TableMeta, selection: Option<&Expr>) -> Option<GapLockKey> {
    let pk_col = single_pk_column(meta)?;
    let filter = parse_where_filter(selection).ok().flatten()?;
    let (low, low_inclusive, high, high_inclusive) = pk_range_from_filter(&filter, &pk_col)?;
    Some(GapLockKey {
        table: String::new(), // filled by caller
        low: Some(low),
        low_inclusive,
        high: Some(high),
        high_inclusive,
    })
}

fn acquire_keys(session: &Session, keys: &[RowKey]) -> Result<(), ExecError> {
    let Some(reg) = registry() else {
        return Ok(());
    };
    wait_on_registry_ids(reg.as_ref(), session.id, lock_wait_timeout(session), keys)
}

fn acquire_gap(session: &Session, table: &str, gap: &GapLockKey) -> Result<(), ExecError> {
    let Some(reg) = registry() else {
        return Ok(());
    };
    let mut gap = gap.clone();
    gap.table = table.to_string();
    let note = rusql_storage::TxnLockNote::Gap {
        table: gap.table.clone(),
        low: gap.low.clone().unwrap_or_else(|| "-".to_string()),
        high: gap.high.clone().unwrap_or_else(|| "-".to_string()),
    };
    let target = LockTarget::Gap(gap);
    if let Err(err) = reg.acquire_exclusive(session.id, &target, lock_wait_timeout(session)) {
        return Err(row_lock_acquire_error(err));
    }
    rusql_storage::note_open_txn_lock(session.id, note);
    Ok(())
}

/// Take exclusive row locks (and RR/SER gap lock for PK range scans) for `FOR UPDATE` rows.
pub(crate) fn acquire_for_update_scan(
    session: &Session,
    table: &str,
    meta: &TableMeta,
    rows: &[Row],
    selection: Option<&Expr>,
) -> Result<(), ExecError> {
    let mut keys = Vec::with_capacity(rows.len());
    for row in rows {
        if let Some(pk) = pk_values_for_row(meta, row) {
            keys.push(RowKey::new(table, pk));
        }
    }
    acquire_keys(session, &keys)?;
    if gap_locks_enabled(session) {
        if let Some(mut gap) = pk_range_gap_lock(meta, selection) {
            gap.table = table.to_string();
            acquire_gap(session, table, &gap)?;
        }
    }
    Ok(())
}

/// Take exclusive row locks for rows returned by a base-table scan (`FOR UPDATE`).
pub(crate) fn acquire_for_update_rows(
    session: &Session,
    table: &str,
    meta: &TableMeta,
    rows: &[Row],
) -> Result<(), ExecError> {
    acquire_for_update_scan(session, table, meta, rows, None)
}

/// Literal `INSERT … VALUES` primary keys to wait on before taking the engine lock (M161).
pub fn collect_insert_lock_pks(
    session: &Session,
    plans: &[Plan],
) -> Result<Vec<(String, String)>, ExecError> {
    let mut pks = Vec::new();
    for plan in plans {
        let Plan::Statement(Statement::Insert(insert)) = plan else {
            continue;
        };
        if insert.ignore || insert.replace_into {
            continue;
        }
        let table = resolve_object_storage_key(session, &insert.table_name)?;
        let Some(meta) = session.catalog.get_table(&table).cloned() else {
            continue;
        };
        let Some(pk_name) = single_pk_column(&meta) else {
            continue;
        };
        let pk_pos = meta
            .columns
            .iter()
            .position(|c| c.name == pk_name)
            .unwrap_or(0);
        let value_indices: Vec<usize> = if insert.columns.is_empty() {
            (0..meta.columns.len()).collect()
        } else {
            insert
                .columns
                .iter()
                .map(|ident| {
                    meta.columns
                        .iter()
                        .position(|c| c.name.eq_ignore_ascii_case(&ident.value))
                        .ok_or_else(|| {
                            ExecError::Message(format!(
                                "Unknown column '{}' in INSERT",
                                ident.value
                            ))
                        })
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let Some(pk_value_idx) = value_indices.iter().position(|&i| i == pk_pos) else {
            continue;
        };
        let Some(query) = insert.source.as_ref() else {
            continue;
        };
        let SetExpr::Values(values) = query.body.as_ref() else {
            continue;
        };
        for row in &values.rows {
            let Some(expr) = row.get(pk_value_idx) else {
                continue;
            };
            let pk = crate::where_filter::literal_expr_to_string(expr)?;
            pks.push((table.clone(), pk));
        }
    }
    Ok(pks)
}

/// Block on gap / row locks before INSERT touches storage (server uses `spawn_blocking`).
pub fn wait_on_insert_pks(
    reg: &RowLockRegistry,
    session_id: u64,
    timeout: Duration,
    pks: &[(String, String)],
) -> Result<(), ExecError> {
    for (table, pk) in pks {
        if let Err(err) = reg.acquire_insert_pk(session_id, table, pk, timeout) {
            return Err(row_lock_acquire_error(err));
        }
        rusql_storage::note_open_txn_lock(
            session_id,
            rusql_storage::TxnLockNote::Record {
                table: table.clone(),
                pk: pk.clone(),
            },
        );
    }
    Ok(())
}

/// Take exclusive row locks before UPDATE touches matching rows.
pub(crate) fn acquire_update_row_locks(
    session: &Session,
    table: &str,
    meta: &TableMeta,
    rows: &[Row],
) -> Result<(), ExecError> {
    acquire_for_update_rows(session, table, meta, rows)
}

pub fn collect_update_lock_keys<E: StorageEngine>(
    engine: &E,
    session: &Session,
    plans: &[Plan],
) -> Result<Vec<RowKey>, ExecError> {
    let mut keys = Vec::new();
    for plan in plans {
        let Plan::Statement(stmt) = plan;
        if let Statement::Update {
            table,
            selection,
            from,
            ..
        } = stmt
        {
            if from.is_some() {
                continue;
            }
            let table_name = resolve_table_with_joins(session, table)?;
            let meta = session
                .catalog
                .get_table(&table_name)
                .cloned()
                .ok_or_else(|| {
                    ExecError::Storage(rusql_storage::StorageError::table_not_found(&table_name))
                })?;
            let filter = extract_eq_predicate(selection.as_ref())
                .map(|(column, value)| rusql_storage::DeleteFilter { column, value });
            let rows = matching_rows(engine, &meta, filter.as_ref())?;
            for row in &rows {
                if let Some(pk) = pk_values_for_row(&meta, row) {
                    keys.push(RowKey::new(table_name.as_str(), pk));
                }
            }
        }
    }
    Ok(keys)
}

/// Block on row locks (call from a blocking thread during async request handling).
pub fn wait_on_registry_ids(
    reg: &RowLockRegistry,
    session_id: u64,
    timeout: Duration,
    keys: &[RowKey],
) -> Result<(), ExecError> {
    let mut sorted = keys.to_vec();
    sorted.sort_by(|a, b| a.table.cmp(&b.table).then(a.pk_values.cmp(&b.pk_values)));
    for key in sorted {
        let note = rusql_storage::TxnLockNote::Record {
            table: key.table.clone(),
            pk: key.pk_values.join(","),
        };
        let target = LockTarget::Row(key);
        if let Err(err) = reg.acquire_exclusive(session_id, &target, timeout) {
            return Err(row_lock_acquire_error(err));
        }
        rusql_storage::note_open_txn_lock(session_id, note);
    }
    Ok(())
}

pub fn lock_wait_timeout_for(session: &Session) -> Duration {
    lock_wait_timeout(session)
}

pub fn row_lock_acquire_error(err: RowLockAcquireError) -> ExecError {
    match err {
        RowLockAcquireError::Timeout(_) => ExecError::Mysql {
            code: 1205,
            message: rusql_i18n::messages::sql_lock_wait_timeout(),
        },
        RowLockAcquireError::Deadlock(_) => ExecError::Mysql {
            code: 1213,
            message: rusql_i18n::messages::sql_deadlock_found(),
        },
    }
}

pub fn exec_error_is_deadlock(error: &ExecError) -> bool {
    matches!(error, ExecError::Mysql { code: 1213, .. })
}

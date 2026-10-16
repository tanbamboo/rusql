//! MySQL `RENAME TABLE` multi-pair execution (M155).

use rusql_core::{table_storage_key, Session};
use rusql_storage::{StorageEngine, StorageError};

use crate::{ExecError, QueryResult};

pub(crate) fn resolve_table_storage_name(
    session: &Session,
    name: &str,
) -> Result<String, ExecError> {
    let parts: Vec<&str> = name.split('.').collect();
    match parts.as_slice() {
        [table] => Ok(table_storage_key(&session.database, table)),
        [schema, table] => Ok(table_storage_key(schema, table)),
        other => Err(ExecError::Message(format!(
            "unsupported table name: {}",
            other.join(".")
        ))),
    }
}

fn catalog_meta_name_for_storage_key(storage_key: &str) -> String {
    storage_key
        .split('.')
        .next_back()
        .unwrap_or(storage_key)
        .to_string()
}

fn catalog_rename_tables(
    session: &mut Session,
    pairs: &[(String, String)],
) -> Result<(), ExecError> {
    let mut staged = Vec::with_capacity(pairs.len());
    for (old, new) in pairs {
        let meta = session
            .catalog
            .get_table(old)
            .cloned()
            .ok_or_else(|| ExecError::Storage(StorageError::table_not_found(old)))?;
        staged.push((old.clone(), new.clone(), meta));
    }
    for (old, new, mut meta) in staged {
        session.catalog.drop_table(&old);
        meta.name = catalog_meta_name_for_storage_key(&new);
        if let Some((schema, _)) = new.split_once('.') {
            meta.schema = schema.to_string();
        }
        session.catalog.create_table(meta);
    }
    Ok(())
}

pub(crate) fn execute_rename_table_pairs<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    raw_pairs: &[(String, String)],
) -> Result<QueryResult, ExecError> {
    let mut pairs = Vec::with_capacity(raw_pairs.len());
    for (old, new) in raw_pairs {
        pairs.push((
            resolve_table_storage_name(session, old)?,
            resolve_table_storage_name(session, new)?,
        ));
    }
    engine
        .rename_tables(&pairs)
        .map_err(exec_error_from_storage)?;
    catalog_rename_tables(session, &pairs)?;
    Ok(QueryResult::Ok { rows_affected: 0 })
}

pub(crate) fn exec_error_from_storage(err: StorageError) -> ExecError {
    let msg = err.to_string();
    if msg.contains("already exists") || msg.contains("已存在") {
        ExecError::Mysql {
            code: 1050,
            message: msg,
        }
    } else {
        ExecError::Storage(err)
    }
}

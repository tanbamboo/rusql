//! FOREIGN KEY validation and enforcement.

use rusql_core::{table_storage_key, ForeignKeyMeta, Session, TableMeta};
use rusql_storage::{ColumnAssignment, DeleteFilter, Row, StorageEngine};
use sqlparser::ast::{ObjectName, ReferentialAction, TableConstraint};

use crate::ExecError;

const SUPPORTED_FK_ACTIONS: &[&str] = &["RESTRICT", "NO ACTION"];

pub fn ref_action_to_string(action: Option<&ReferentialAction>) -> String {
    action
        .map(|a| a.to_string())
        .unwrap_or_else(|| "RESTRICT".into())
}

pub fn ensure_supported_fk_action(action: &str, kind: &str) -> Result<(), ExecError> {
    if SUPPORTED_FK_ACTIONS.contains(&action) {
        Ok(())
    } else {
        Err(ExecError::Message(format!(
            "unsupported FOREIGN KEY ON {kind} {action}"
        )))
    }
}

pub fn foreign_key_from_constraint(
    constraint: &TableConstraint,
    child_schema: &str,
) -> Result<Option<ForeignKeyMeta>, ExecError> {
    let TableConstraint::ForeignKey {
        name,
        columns,
        foreign_table,
        referred_columns,
        on_delete,
        on_update,
        ..
    } = constraint
    else {
        return Ok(None);
    };
    if columns.len() != referred_columns.len() {
        return Err(ExecError::Message(
            "FOREIGN KEY column count must match referenced column count".into(),
        ));
    }
    let (referenced_schema, referenced_table, _) =
        resolve_fk_reference(child_schema, foreign_table)?;
    let on_delete = ref_action_to_string(on_delete.as_ref());
    let on_update = ref_action_to_string(on_update.as_ref());
    ensure_supported_fk_action(&on_delete, "DELETE")?;
    ensure_supported_fk_action(&on_update, "UPDATE")?;
    Ok(Some(ForeignKeyMeta {
        name: name.as_ref().map(|i| i.value.clone()),
        columns: columns.iter().map(|i| i.value.clone()).collect(),
        referenced_schema,
        referenced_table,
        referenced_columns: referred_columns.iter().map(|i| i.value.clone()).collect(),
        on_delete,
        on_update,
    }))
}

pub fn resolve_fk_reference(
    default_schema: &str,
    name: &ObjectName,
) -> Result<(String, String, String), ExecError> {
    let parts: Vec<_> = name.0.iter().map(|i| i.value.clone()).collect();
    match parts.as_slice() {
        [table] => Ok((
            default_schema.to_string(),
            table.clone(),
            table_storage_key(default_schema, table),
        )),
        [schema, table] => Ok((
            schema.clone(),
            table.clone(),
            table_storage_key(schema, table),
        )),
        other => Err(ExecError::Message(format!(
            "unsupported foreign key reference: {}",
            other.join(".")
        ))),
    }
}

pub fn validate_foreign_keys(session: &Session, meta: &TableMeta) -> Result<(), ExecError> {
    for fk in &meta.foreign_keys {
        let parent_key = table_storage_key(&fk.referenced_schema, &fk.referenced_table);
        let parent = session.catalog.get_table(&parent_key).ok_or_else(|| {
            ExecError::Message(format!(
                "Failed to add the foreign key constraint. Missing index for constraint '{}' in the referenced table '{}'",
                fk.constraint_name(&meta.name, 0),
                fk.referenced_table
            ))
        })?;
        for col in &fk.columns {
            if !meta
                .columns
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(col))
            {
                return Err(ExecError::Message(format!(
                    "Key column '{col}' doesn't exist in table"
                )));
            }
        }
        for col in &fk.referenced_columns {
            if !parent
                .columns
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(col))
            {
                return Err(ExecError::Message(format!(
                    "Key column '{col}' doesn't exist in table"
                )));
            }
        }
    }
    Ok(())
}

pub fn matching_rows<E: StorageEngine>(
    engine: &E,
    meta: &TableMeta,
    filter: Option<&DeleteFilter>,
) -> Result<Vec<Row>, ExecError> {
    let key = table_storage_key(&meta.schema, &meta.name);
    match filter {
        None => Ok(engine.scan(&key)?),
        Some(f) => {
            if let Some(rows) = engine.scan_eq(&key, &f.column, &f.value)? {
                return Ok(rows);
            }
            let rows = engine.scan(&key)?;
            let idx = column_index(meta, &f.column)?;
            Ok(rows
                .into_iter()
                .filter(|r| r.get(idx).is_some_and(|v| v == &f.value))
                .collect())
        }
    }
}

pub fn check_insert<E: StorageEngine>(
    engine: &E,
    session: &Session,
    child_meta: &TableMeta,
    row: &Row,
) -> Result<(), ExecError> {
    for (i, fk) in child_meta.foreign_keys.iter().enumerate() {
        let child_vals = fk_column_values(child_meta, fk, row)?;
        if child_vals.iter().all(|v| v.is_empty()) {
            continue;
        }
        if !parent_row_exists(engine, session, fk, &child_vals)? {
            return Err(child_violation(child_meta, fk, i));
        }
    }
    Ok(())
}

pub fn check_delete<E: StorageEngine>(
    engine: &E,
    session: &Session,
    parent_meta: &TableMeta,
    rows: &[Row],
) -> Result<(), ExecError> {
    for row in rows {
        for (child_meta, fk, fk_index) in incoming_foreign_keys(session, parent_meta)? {
            let parent_vals = referenced_values_in_parent(parent_meta, &fk, row)?;
            if parent_vals.iter().all(|v| v.is_empty()) {
                continue;
            }
            if child_has_reference(engine, &child_meta, &fk, &parent_vals)? {
                return Err(parent_violation(&child_meta, &fk, fk_index, parent_meta));
            }
        }
    }
    Ok(())
}

pub fn check_update<E: StorageEngine>(
    engine: &E,
    session: &Session,
    meta: &TableMeta,
    old_row: &Row,
    new_row: &Row,
) -> Result<(), ExecError> {
    check_insert(engine, session, meta, new_row)?;
    for (child_meta, fk, fk_index) in incoming_foreign_keys(session, meta)? {
        let old_vals = referenced_values_in_parent(meta, &fk, old_row)?;
        let new_vals = referenced_values_in_parent(meta, &fk, new_row)?;
        if old_vals == new_vals {
            continue;
        }
        if old_vals.iter().all(|v| v.is_empty()) {
            continue;
        }
        if child_has_reference(engine, &child_meta, &fk, &old_vals)? {
            return Err(parent_violation(&child_meta, &fk, fk_index, meta));
        }
    }
    Ok(())
}

pub fn apply_assignments(
    meta: &TableMeta,
    row: &Row,
    assigns: &[ColumnAssignment],
) -> Result<Row, ExecError> {
    let mut out = row.to_vec();
    for a in assigns {
        let idx = column_index(meta, &a.column)?;
        if let Some(slot) = out.get_mut(idx) {
            *slot = a.value.clone();
        }
    }
    Ok(out)
}

fn incoming_foreign_keys(
    session: &Session,
    parent_meta: &TableMeta,
) -> Result<Vec<(TableMeta, ForeignKeyMeta, usize)>, ExecError> {
    let mut out = Vec::new();
    for child in session.catalog.iter_tables() {
        for (i, fk) in child.foreign_keys.iter().enumerate() {
            if fk.referenced_schema == parent_meta.schema
                && fk.referenced_table.eq_ignore_ascii_case(&parent_meta.name)
            {
                out.push((child.clone(), fk.clone(), i));
            }
        }
    }
    Ok(out)
}

fn referenced_values_in_parent(
    parent_meta: &TableMeta,
    fk: &ForeignKeyMeta,
    row: &Row,
) -> Result<Vec<String>, ExecError> {
    let mut out = Vec::with_capacity(fk.referenced_columns.len());
    for col in &fk.referenced_columns {
        let idx = column_index(parent_meta, col)?;
        out.push(row.get(idx).cloned().unwrap_or_default());
    }
    Ok(out)
}

fn fk_column_values(
    meta: &TableMeta,
    fk: &ForeignKeyMeta,
    row: &Row,
) -> Result<Vec<String>, ExecError> {
    let mut out = Vec::with_capacity(fk.columns.len());
    for col in &fk.columns {
        let idx = column_index(meta, col)?;
        out.push(row.get(idx).cloned().unwrap_or_default());
    }
    Ok(out)
}

fn parent_row_exists<E: StorageEngine>(
    engine: &E,
    session: &Session,
    fk: &ForeignKeyMeta,
    child_vals: &[String],
) -> Result<bool, ExecError> {
    let parent_key = table_storage_key(&fk.referenced_schema, &fk.referenced_table);
    let parent = session
        .catalog
        .get_table(&parent_key)
        .ok_or_else(|| ExecError::Message(format!("referenced table '{}' missing", parent_key)))?;
    let parent_rows = engine.scan(&parent_key)?;
    for parent_row in parent_rows {
        let mut matches = true;
        for (col, val) in fk.referenced_columns.iter().zip(child_vals) {
            let idx = column_index(parent, col)?;
            if parent_row.get(idx) != Some(val) {
                matches = false;
                break;
            }
        }
        if matches {
            return Ok(true);
        }
    }
    Ok(false)
}

fn child_has_reference<E: StorageEngine>(
    engine: &E,
    child_meta: &TableMeta,
    fk: &ForeignKeyMeta,
    parent_vals: &[String],
) -> Result<bool, ExecError> {
    let child_key = table_storage_key(&child_meta.schema, &child_meta.name);
    let rows = engine.scan(&child_key)?;
    for row in rows {
        let child_vals = fk_column_values(child_meta, fk, &row)?;
        if child_vals == parent_vals {
            return Ok(true);
        }
    }
    Ok(false)
}

fn column_index(meta: &TableMeta, name: &str) -> Result<usize, ExecError> {
    meta.columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| ExecError::Message(format!("unknown column '{name}'")))
}

fn child_violation(meta: &TableMeta, fk: &ForeignKeyMeta, index: usize) -> ExecError {
    ExecError::Mysql {
        code: 1452,
        message: rusql_i18n::messages::sql_fk_child_violation(
            &meta.schema,
            &meta.name,
            &fk.constraint_name(&meta.name, index),
            &fk.columns.join("`, `"),
            &fk.referenced_table,
            &fk.referenced_columns.join("`, `"),
        ),
    }
}

fn parent_violation(
    child_meta: &TableMeta,
    fk: &ForeignKeyMeta,
    index: usize,
    parent_meta: &TableMeta,
) -> ExecError {
    ExecError::Mysql {
        code: 1451,
        message: rusql_i18n::messages::sql_fk_parent_violation(
            &child_meta.schema,
            &child_meta.name,
            &fk.constraint_name(&child_meta.name, index),
            &fk.columns.join("`, `"),
            &parent_meta.name,
            &fk.referenced_columns.join("`, `"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::ColumnDef;
    use rusql_storage::{HeapEngine, StorageEngine};

    fn parent_child_catalog() -> (Session, HeapEngine) {
        let mut session = Session::new(1, "root");
        let mut eng = HeapEngine::new();
        let parent = TableMeta {
            name: "parent".into(),
            schema: "rusql".into(),
            columns: vec![ColumnDef::new("id", "INT")],
            auto_increment_next: None,
            foreign_keys: vec![],
        };
        let child = TableMeta {
            name: "child".into(),
            schema: "rusql".into(),
            columns: vec![
                ColumnDef::new("id", "INT"),
                ColumnDef::new("parent_id", "INT"),
            ],
            auto_increment_next: None,
            foreign_keys: vec![ForeignKeyMeta {
                name: Some("fk_child_parent".into()),
                columns: vec!["parent_id".into()],
                referenced_schema: "rusql".into(),
                referenced_table: "parent".into(),
                referenced_columns: vec!["id".into()],
                on_delete: "RESTRICT".into(),
                on_update: "RESTRICT".into(),
            }],
        };
        eng.create_table(parent.clone()).unwrap();
        eng.create_table(child.clone()).unwrap();
        session.catalog.create_table(parent);
        session.catalog.create_table(child);
        eng.insert("parent", vec!["1".into()]).unwrap();
        (session, eng)
    }

    #[test]
    fn insert_valid_and_invalid_fk() {
        let (session, eng) = parent_child_catalog();
        let child_meta = session.catalog.get_table("child").unwrap().clone();
        check_insert(&eng, &session, &child_meta, &vec!["1".into(), "1".into()]).unwrap();
        assert!(check_insert(&eng, &session, &child_meta, &vec!["2".into(), "99".into()]).is_err());
    }

    #[test]
    fn delete_parent_with_child_fails() {
        let (session, mut eng) = parent_child_catalog();
        eng.insert("child", vec!["1".into(), "1".into()]).unwrap();
        let parent_meta = session.catalog.get_table("parent").unwrap().clone();
        let rows = matching_rows(&eng, &parent_meta, None).unwrap();
        assert!(check_delete(&eng, &session, &parent_meta, &rows).is_err());
    }
}

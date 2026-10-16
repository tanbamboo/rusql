//! VIRTUAL generated columns (M146): catalog metadata and SELECT-time evaluation.

use crate::expr::eval_expr;
use crate::ExecError;
use rusql_core::{table_storage_key, TableMeta, VirtualGeneratedColumn};
use rusql_sql::parse_generation_expression;
use rusql_storage::{Row, StorageEngine};

const ER_NON_DEFAULT_VALUE_FOR_GENERATED_COLUMN: u16 = 3105;

pub fn write_to_generated_column_error(meta: &TableMeta, column: &str) -> ExecError {
    ExecError::Mysql {
        code: ER_NON_DEFAULT_VALUE_FOR_GENERATED_COLUMN,
        message: rusql_i18n::messages::sql_generated_column_value_not_allowed(column, &meta.name),
    }
}

pub fn stored_generated_unsupported_error() -> ExecError {
    ExecError::Message(rusql_i18n::messages::sql_stored_generated_unsupported())
}

pub fn reject_update_of_generated(meta: &TableMeta, column: &str) -> Result<(), ExecError> {
    let is_generated = meta
        .columns
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(column))
        .is_some_and(|c| c.is_virtual_generated());
    if is_generated {
        return Err(write_to_generated_column_error(meta, column));
    }
    Ok(())
}

pub fn materialize_virtual_columns(
    meta: &TableMeta,
    rows: Vec<Row>,
    session: &rusql_core::Session,
) -> Result<Vec<Row>, ExecError> {
    if !meta.has_virtual_generated_columns() {
        return Ok(rows);
    }
    let column_names: Vec<String> = meta.columns.iter().map(|c| c.name.clone()).collect();
    let mut out = Vec::with_capacity(rows.len());
    for mut row in rows {
        pad_row_width(&mut row, meta.columns.len());
        for (idx, col) in meta.columns.iter().enumerate() {
            let Some(gen) = col.virtual_generated.as_ref() else {
                continue;
            };
            row[idx] = eval_virtual_generated(gen, &row, &column_names, session)?;
        }
        out.push(row);
    }
    Ok(out)
}

fn pad_row_width(row: &mut Row, width: usize) {
    if row.len() < width {
        row.resize(width, String::new());
    }
}

fn eval_virtual_generated(
    gen: &VirtualGeneratedColumn,
    row: &Row,
    column_names: &[String],
    session: &rusql_core::Session,
) -> Result<String, ExecError> {
    let expr = parse_generation_expression(&gen.expression_sql)
        .map_err(|e| ExecError::Message(rusql_i18n::messages::sql_parse_error(&e.to_string())))?;
    eval_expr(row, column_names, &expr, Some(session))
}

#[allow(dead_code)]
pub fn scan_table_rows<E: StorageEngine>(
    engine: &E,
    meta: &TableMeta,
    session: &rusql_core::Session,
) -> Result<Vec<Row>, ExecError> {
    let key = table_storage_key(&meta.schema, &meta.name);
    let rows = engine.scan(&key)?;
    materialize_virtual_columns(meta, rows, session)
}

pub fn is_default_insert_value(value: &str) -> bool {
    value.is_empty() || value.eq_ignore_ascii_case("DEFAULT")
}

/// Reject explicit non-DEFAULT values targeting VIRTUAL generated columns.
pub fn reject_insert_into_generated(
    meta: &TableMeta,
    column_indices: &[usize],
    values: &[String],
) -> Result<(), ExecError> {
    for (&idx, val) in column_indices.iter().zip(values) {
        let col = &meta.columns[idx];
        if col.is_virtual_generated() && !is_default_insert_value(val) {
            return Err(write_to_generated_column_error(meta, &col.name));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusql_core::{ColumnDef, TableMeta, VirtualGeneratedColumn};
    use rusql_storage::{HeapEngine, StorageEngine};

    use super::*;

    fn gen_table() -> TableMeta {
        TableMeta {
            name: "t".into(),
            schema: "rusql".into(),
            columns: vec![
                ColumnDef::new("id", "INT"),
                ColumnDef {
                    name: "n".into(),
                    data_type: "INT".into(),
                    nullable: true,
                    primary_key: false,
                    auto_increment: false,
                    collation: None,
                    virtual_generated: None,
                },
                ColumnDef {
                    name: "g".into(),
                    data_type: "INT".into(),
                    nullable: true,
                    primary_key: false,
                    auto_increment: false,
                    collation: None,
                    virtual_generated: Some(VirtualGeneratedColumn {
                        expression_sql: "n + 1".into(),
                    }),
                },
            ],
            auto_increment_next: None,
            foreign_keys: vec![],
        }
    }

    #[test]
    fn materialize_virtual_generated_column() {
        let meta = gen_table();
        let session = rusql_core::Session::new(1, "root");
        let row = vec!["1".into(), "4".into(), String::new()];
        let out = materialize_virtual_columns(&meta, vec![row], &session).unwrap();
        assert_eq!(out[0][2], "5");
    }

    #[test]
    fn stored_column_indices_skip_virtual() {
        let meta = gen_table();
        assert_eq!(meta.stored_column_indices(), vec![0, 1]);
    }

    #[test]
    fn generated_column_storage_wal_roundtrip() {
        let mut eng = HeapEngine::new();
        let meta = gen_table();
        eng.create_table(meta.clone()).unwrap();
        eng.insert("t", vec!["1".into(), "2".into(), String::new()])
            .unwrap();
        let session = rusql_core::Session::new(1, "root");
        let rows = scan_table_rows(&eng, &meta, &session).unwrap();
        assert_eq!(rows[0][2], "3");
    }
}

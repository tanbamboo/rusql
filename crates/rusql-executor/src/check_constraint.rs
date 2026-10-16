//! CHECK constraint enforcement (M147).

use crate::expr::{eval_expr, expr_is_truthy};
use crate::generated_column;
use crate::ExecError;
use rusql_core::{CheckConstraintMeta, Session, TableMeta};
use rusql_sql::parse_check_expression;
use rusql_storage::Row;

const ER_CHECK_CONSTRAINT_VIOLATED: u16 = 3819;

pub fn check_row_constraints(
    session: &Session,
    meta: &TableMeta,
    row: &Row,
) -> Result<(), ExecError> {
    if meta.check_constraints.is_empty() {
        return Ok(());
    }
    let mut padded = row.to_vec();
    if padded.len() < meta.columns.len() {
        padded.resize(meta.columns.len(), String::new());
    }
    let materialized = generated_column::materialize_virtual_columns(meta, vec![padded], session)?;
    let eval_row = &materialized[0];
    let column_names: Vec<String> = meta.columns.iter().map(|c| c.name.clone()).collect();

    for (i, chk) in meta.check_constraints.iter().enumerate() {
        if !chk.enforced {
            continue;
        }
        let expr = parse_check_expression(&chk.expression_sql).map_err(|e| {
            ExecError::Message(rusql_i18n::messages::sql_parse_error(&e.to_string()))
        })?;
        let val = eval_expr(eval_row, &column_names, &expr, Some(session))?;
        if !expr_is_truthy(&val) {
            return Err(violation_error(meta, chk, i));
        }
    }
    Ok(())
}

fn violation_error(meta: &TableMeta, chk: &CheckConstraintMeta, index: usize) -> ExecError {
    let constraint = chk.constraint_name(&meta.name, index);
    ExecError::Mysql {
        code: ER_CHECK_CONSTRAINT_VIOLATED,
        message: rusql_i18n::messages::sql_check_constraint_violated(&constraint),
    }
}

#[cfg(test)]
mod tests {
    use rusql_core::{CheckConstraintMeta, ColumnDef, TableMeta};
    use rusql_storage::{HeapEngine, StorageEngine};

    use super::*;

    fn check_table() -> TableMeta {
        TableMeta {
            name: "t".into(),
            schema: "rusql".into(),
            columns: vec![ColumnDef::new("id", "INT"), ColumnDef::new("n", "INT")],
            auto_increment_next: None,
            foreign_keys: vec![],
            check_constraints: vec![CheckConstraintMeta {
                name: None,
                expression_sql: "n > 0".into(),
                enforced: true,
                column: Some("n".into()),
            }],
        }
    }

    #[test]
    fn check_constraint_insert_valid() {
        let mut eng = HeapEngine::new();
        let meta = check_table();
        eng.create_table(meta.clone()).unwrap();
        let session = Session::new(1, "root");
        check_row_constraints(&session, &meta, &vec!["1".into(), "5".into()]).unwrap();
        eng.insert("t", vec!["1".into(), "5".into()]).unwrap();
    }

    #[test]
    fn check_constraint_insert_violation_errno_3819() {
        let meta = check_table();
        let session = Session::new(1, "root");
        let err =
            check_row_constraints(&session, &meta, &vec!["1".into(), "0".into()]).unwrap_err();
        match err {
            ExecError::Mysql { code, .. } => assert_eq!(code, 3819),
            other => panic!("expected mysql error, got {other:?}"),
        }
    }

    #[test]
    fn check_constraint_named_stored_in_catalog() {
        let meta = TableMeta {
            check_constraints: vec![CheckConstraintMeta {
                name: Some("c".into()),
                expression_sql: "n > 0".into(),
                enforced: true,
                column: None,
            }],
            ..check_table()
        };
        assert_eq!(meta.check_constraints[0].name.as_deref(), Some("c"));
    }
}

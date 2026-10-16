//! Column DEFAULT expressions (M149): catalog metadata and INSERT/UPDATE evaluation.

use crate::expr::eval_expr;
use crate::ExecError;
use rusql_core::{ColumnDef, ColumnDefault, TableMeta};
use rusql_sql::parse_default_expression;
use rusql_storage::{ColumnAssignment, Row};
use sqlparser::ast::Expr;
use std::sync::Arc;

pub fn column_default_from_ast(expr: &Expr) -> Result<ColumnDefault, ExecError> {
    let (expression_sql, parenthesized) = match expr {
        Expr::Nested(inner) => (inner.to_string(), true),
        other => (other.to_string(), false),
    };
    parse_default_expression(&expression_sql)
        .map_err(|e| ExecError::Message(rusql_i18n::messages::sql_parse_error(&e.to_string())))?;
    Ok(ColumnDefault {
        expression_sql,
        parenthesized,
    })
}

pub fn reject_default_on_generated(col: &ColumnDef) -> Result<(), ExecError> {
    if col.is_virtual_generated() && col.default.is_some() {
        return Err(ExecError::Message(
            rusql_i18n::messages::sql_default_on_generated_column(&col.name),
        ));
    }
    Ok(())
}

pub fn format_default_for_show(def: &ColumnDefault) -> String {
    if def.parenthesized {
        format!(" DEFAULT ({})", def.expression_sql.trim())
    } else {
        format!(" DEFAULT {}", def.expression_sql.trim())
    }
}

pub fn describe_default_value(def: &ColumnDefault) -> String {
    def.expression_sql.trim().to_string()
}

pub fn eval_column_default(
    def: &ColumnDefault,
    row: &Row,
    column_names: &[String],
    session: Option<&rusql_core::Session>,
) -> Result<String, ExecError> {
    let expr = parse_default_expression(&def.expression_sql)
        .map_err(|e| ExecError::Message(rusql_i18n::messages::sql_parse_error(&e.to_string())))?;
    eval_expr(row, column_names, &expr, session)
}

pub fn apply_insert_column_defaults(
    session: &rusql_core::Session,
    meta: &TableMeta,
    row: &mut Row,
    provided: &mut [bool],
) -> Result<(), ExecError> {
    let column_names: Arc<[String]> = meta.columns.iter().map(|c| c.name.clone()).collect();
    for (i, col) in meta.columns.iter().enumerate() {
        if col.is_virtual_generated() {
            continue;
        }
        let needs = !provided[i] || crate::generated_column::is_default_insert_value(&row[i]);
        if !needs {
            continue;
        }
        let Some(def) = col.default.as_ref() else {
            continue;
        };
        row[i] = eval_column_default(def, row, &column_names, Some(session))?;
        provided[i] = true;
    }
    Ok(())
}

pub fn is_default_keyword_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Identifier(id) if id.value.eq_ignore_ascii_case("DEFAULT")
    )
}

pub fn resolve_update_assignment(
    meta: &TableMeta,
    row: &Row,
    assignment: &ColumnAssignment,
    session: &rusql_core::Session,
) -> Result<String, ExecError> {
    if !assignment.use_column_default {
        return Ok(assignment.value.clone());
    }
    let col = meta
        .columns
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(&assignment.column))
        .ok_or_else(|| {
            ExecError::Message(format!("Unknown column '{}' in UPDATE", assignment.column))
        })?;
    let def = col.default.as_ref().ok_or_else(|| {
        ExecError::Message(rusql_i18n::messages::sql_column_has_no_default(
            &assignment.column,
        ))
    })?;
    let column_names: Vec<String> = meta.columns.iter().map(|c| c.name.clone()).collect();
    eval_column_default(def, row, &column_names, Some(session))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_planner::plan;
    use rusql_sql::parse;
    use rusql_storage::HeapEngine;

    fn exec_ok(
        exec: &mut crate::Executor<HeapEngine>,
        session: &mut rusql_core::Session,
        sql: &str,
    ) {
        let plans = plan(session, parse(sql).unwrap());
        let results = exec.execute(session, &plans, None).unwrap();
        assert!(
            matches!(results[0], crate::QueryResult::Ok { rows_affected: _ }),
            "expected OK for {sql}, got {:?}",
            results[0]
        );
    }

    fn query_rows(
        exec: &mut crate::Executor<HeapEngine>,
        session: &mut rusql_core::Session,
        sql: &str,
    ) -> Vec<Row> {
        let plans = plan(session, parse(sql).unwrap());
        let results = exec.execute(session, &plans, None).unwrap();
        match &results[0] {
            crate::QueryResult::Rows { rows, .. } => rows.clone(),
            other => panic!("expected rows for {sql}, got {other:?}"),
        }
    }

    #[test]
    fn default_expr_insert_omit_evaluates_arithmetic() {
        let mut exec = crate::Executor::new(HeapEngine::new());
        let mut session = rusql_core::Session::new(1, "root");
        exec_ok(
            &mut exec,
            &mut session,
            "CREATE TABLE m149_t (id INT PRIMARY KEY, n INT DEFAULT (1+2))",
        );
        exec_ok(
            &mut exec,
            &mut session,
            "INSERT INTO m149_t (id) VALUES (1)",
        );
        let rows = query_rows(&mut exec, &mut session, "SELECT id, n FROM m149_t");
        assert_eq!(rows[0][1], "3");
    }

    #[test]
    fn default_expr_literal_insert_omit() {
        let mut exec = crate::Executor::new(HeapEngine::new());
        let mut session = rusql_core::Session::new(1, "root");
        exec_ok(
            &mut exec,
            &mut session,
            "CREATE TABLE m149_lit (id INT PRIMARY KEY, n INT DEFAULT 7)",
        );
        exec_ok(
            &mut exec,
            &mut session,
            "INSERT INTO m149_lit (id) VALUES (1)",
        );
        let rows = query_rows(&mut exec, &mut session, "SELECT n FROM m149_lit");
        assert_eq!(rows[0][0], "7");
    }

    #[test]
    fn default_expr_current_timestamp_on_datetime() {
        let mut exec = crate::Executor::new(HeapEngine::new());
        let mut session = rusql_core::Session::new(1, "root");
        exec_ok(
            &mut exec,
            &mut session,
            "CREATE TABLE m149_ts (id INT PRIMARY KEY, ts DATETIME DEFAULT CURRENT_TIMESTAMP)",
        );
        exec_ok(
            &mut exec,
            &mut session,
            "INSERT INTO m149_ts (id) VALUES (1)",
        );
        let rows = query_rows(&mut exec, &mut session, "SELECT ts FROM m149_ts");
        assert!(
            !rows[0][0].is_empty(),
            "CURRENT_TIMESTAMP default should produce a datetime string"
        );
    }

    #[test]
    fn default_expr_invalid_at_create_errors() {
        assert!(parse("CREATE TABLE bad_def (id INT, n INT DEFAULT (1 +))").is_err());
    }

    #[test]
    fn default_expr_update_set_default() {
        let mut exec = crate::Executor::new(HeapEngine::new());
        let mut session = rusql_core::Session::new(1, "root");
        exec_ok(
            &mut exec,
            &mut session,
            "CREATE TABLE m149_u (id INT PRIMARY KEY, n INT DEFAULT (10))",
        );
        exec_ok(
            &mut exec,
            &mut session,
            "INSERT INTO m149_u (id, n) VALUES (1, 99)",
        );
        exec_ok(
            &mut exec,
            &mut session,
            "UPDATE m149_u SET n = DEFAULT WHERE id = 1",
        );
        let rows = query_rows(&mut exec, &mut session, "SELECT n FROM m149_u");
        assert_eq!(rows[0][0], "10");
    }

    #[test]
    fn show_create_table_includes_default_expr() {
        let mut exec = crate::Executor::new(HeapEngine::new());
        let mut session = rusql_core::Session::new(1, "root");
        exec_ok(
            &mut exec,
            &mut session,
            "CREATE TABLE m149_show (id INT PRIMARY KEY, n INT DEFAULT (1+2))",
        );
        let rows = query_rows(&mut exec, &mut session, "SHOW CREATE TABLE m149_show");
        assert!(rows[0][1].contains("DEFAULT (1 + 2)"));
    }
}

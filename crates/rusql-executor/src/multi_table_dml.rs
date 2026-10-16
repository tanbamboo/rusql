//! M144: two-table INNER JOIN forms of UPDATE and DELETE.

use crate::enum_set;
use crate::expr::eval_expr;
use crate::fk::{apply_assignments, check_delete, check_update};
use crate::programs::{apply_after_delete_triggers, apply_after_update_triggers};
use crate::subquery::{filter_inline_rows, parse_where_with_subqueries};
use crate::{ExecError, QueryResult};
use rusql_core::{PrivilegeStore, Session, TableMeta};
use rusql_storage::{ColumnAssignment, DeleteFilter, Row, StorageEngine};
use sqlparser::ast::{
    Assignment, AssignmentTarget, Delete, Expr, FromTable, JoinConstraint, JoinOperator,
    ObjectName, TableFactor, TableWithJoins,
};

pub(crate) fn update_has_inner_join(table: &TableWithJoins) -> bool {
    !table.joins.is_empty()
}

pub(crate) fn delete_has_inner_join(delete: &Delete) -> bool {
    let tables = delete_from_tables(delete);
    tables.first().is_some_and(|t| !t.joins.is_empty())
}

fn delete_from_tables(delete: &Delete) -> &[TableWithJoins] {
    match &delete.from {
        FromTable::WithFromKeyword(t) | FromTable::WithoutKeyword(t) => t.as_slice(),
    }
}

/// Physical storage key for privilege checks and DML.
pub(crate) fn delete_target_storage_key(
    session: &Session,
    delete: &Delete,
) -> Result<String, ExecError> {
    if delete.tables.len() > 1 {
        return Err(ExecError::Message(
            "multi-target DELETE is not supported".into(),
        ));
    }
    if let Some(target) = delete.tables.first() {
        if delete_has_inner_join(delete) {
            let from = delete_from_tables(delete)
                .first()
                .ok_or_else(|| ExecError::Message("DELETE requires a table".into()))?;
            let (left, right, _) = parse_inner_join(from, session)?;
            if side_matches(&left, &object_name_to_string(target)) {
                return Ok(left.table_name);
            }
            if side_matches(&right, &object_name_to_string(target)) {
                return Ok(right.table_name);
            }
            return Err(ExecError::Message(format!(
                "unknown table '{}'",
                object_name_to_string(target)
            )));
        }
        return crate::resolve_object_storage_key(session, target);
    }
    let first = delete_from_tables(delete)
        .first()
        .ok_or_else(|| ExecError::Message("DELETE requires a table".into()))?;
    match &first.relation {
        TableFactor::Table { name, .. } => crate::resolve_object_storage_key(session, name),
        other => Err(ExecError::Message(format!(
            "unsupported DELETE target: {other:?}"
        ))),
    }
}

pub(crate) fn execute_update_inner_join<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    table: &TableWithJoins,
    assignments: &[Assignment],
    selection: Option<&Expr>,
    privileges: &PrivilegeStore,
) -> Result<QueryResult, ExecError> {
    if assignments.is_empty() {
        return Ok(QueryResult::Ok { rows_affected: 0 });
    }
    let (left, right, key) = parse_inner_join(table, session)?;
    let left_meta = table_meta(session, &left.table_name)?;
    let assigns_template = parse_update_assignments(assignments, &left, &left_meta)?;

    let left_rows = engine.scan(&left.table_name)?;
    let right_rows = engine.scan(&right.table_name)?;
    let (_combined_columns, mut joined) = crate::nested_loop_join(
        crate::JoinKind::Inner,
        &left,
        left_rows,
        &right,
        right_rows,
        &key,
    )?;
    let qualified = qualified_join_columns(&left, &right);
    if let Some(filter) = parse_where_with_subqueries(selection)? {
        joined = filter_inline_rows(engine, session, joined, &qualified, &filter, &[])?;
    }

    let pk = single_pk_column(&left_meta)?;
    let mut affected = 0u64;
    for row in joined {
        let mut assigns = Vec::with_capacity(assigns_template.len());
        for (col, value_expr) in &assigns_template {
            let value = eval_join_expr(&row, &left, &right, value_expr, Some(session))?;
            assigns.push(ColumnAssignment {
                column: col.clone(),
                value,
                use_column_default: false,
            });
        }
        let left_row: Row = left_row_slice(&row, left.columns.len()).to_vec();
        enum_set::coerce_column_assignments(session, &left_meta, &mut assigns)?;
        let new_row = apply_assignments(&left_meta, &left_row, &assigns)?;
        check_update(engine, session, &left_meta, &left_row, &new_row)?;
        let filter = DeleteFilter {
            column: pk.clone(),
            value: row_cell(&left_meta, &left_row, &pk)?,
        };
        if !assigns.is_empty() {
            affected += engine.update_rows(&left.table_name, &assigns, Some(filter))?;
        }
        apply_after_update_triggers(
            engine,
            session,
            &left_meta,
            &left_row,
            &new_row,
            Some(privileges),
        )?;
    }
    Ok(QueryResult::Ok {
        rows_affected: affected,
    })
}

pub(crate) fn execute_delete_inner_join<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    delete: &Delete,
    privileges: &PrivilegeStore,
) -> Result<QueryResult, ExecError> {
    if delete.limit.is_some() {
        return Err(ExecError::Message(
            "LIMIT on multi-table DELETE is not supported".into(),
        ));
    }
    if delete.tables.len() > 1 {
        return Err(ExecError::Message(
            "multi-target DELETE is not supported".into(),
        ));
    }
    let target_name = delete
        .tables
        .first()
        .ok_or_else(|| ExecError::Message("DELETE requires a target table".into()))?;
    let from = delete_from_tables(delete)
        .first()
        .ok_or_else(|| ExecError::Message("DELETE requires a table".into()))?;
    let (left, right, key) = parse_inner_join(from, session)?;
    let delete_side = if side_matches(&left, &object_name_to_string(target_name)) {
        left.clone()
    } else if side_matches(&right, &object_name_to_string(target_name)) {
        right.clone()
    } else {
        return Err(ExecError::Message(format!(
            "unknown table '{}'",
            object_name_to_string(target_name)
        )));
    };
    let meta = table_meta(session, &delete_side.table_name)?;

    let left_rows = engine.scan(&left.table_name)?;
    let right_rows = engine.scan(&right.table_name)?;
    let (_combined_columns, mut joined) = crate::nested_loop_join(
        crate::JoinKind::Inner,
        &left,
        left_rows,
        &right,
        right_rows,
        &key,
    )?;
    let qualified = qualified_join_columns(&left, &right);
    if let Some(filter) = parse_where_with_subqueries(delete.selection.as_ref())? {
        joined = filter_inline_rows(engine, session, joined, &qualified, &filter, &[])?;
    }

    let pk = single_pk_column(&meta)?;
    let mut affected = 0u64;
    for row in joined {
        let side_len = if delete_side.table_name == left.table_name {
            left.columns.len()
        } else {
            right.columns.len()
        };
        let target_row: Row = if delete_side.table_name == left.table_name {
            left_row_slice(&row, side_len).to_vec()
        } else {
            row[left.columns.len()..left.columns.len() + side_len].to_vec()
        };
        check_delete(engine, session, &meta, std::slice::from_ref(&target_row))?;
        apply_after_delete_triggers(engine, session, &meta, &target_row, Some(privileges))?;
        let filter = DeleteFilter {
            column: pk.clone(),
            value: row_cell(&meta, &target_row, &pk)?,
        };
        affected += engine.delete_rows(&delete_side.table_name, Some(filter))?;
    }
    Ok(QueryResult::Ok {
        rows_affected: affected,
    })
}

fn parse_update_assignments(
    assignments: &[Assignment],
    update_side: &crate::JoinSide,
    meta: &TableMeta,
) -> Result<Vec<(String, Expr)>, ExecError> {
    let mut out = Vec::with_capacity(assignments.len());
    for a in assignments {
        let column = assignment_target_column(&a.target, update_side)?;
        if !meta
            .columns
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&column))
        {
            return Err(ExecError::Message(format!("Unknown column '{column}'")));
        }
        out.push((column, a.value.clone()));
    }
    Ok(out)
}

fn assignment_target_column(
    target: &AssignmentTarget,
    update_side: &crate::JoinSide,
) -> Result<String, ExecError> {
    match target {
        AssignmentTarget::ColumnName(name) => {
            let parts: Vec<_> = name.0.iter().map(|i| i.value.clone()).collect();
            match parts.as_slice() {
                [col] => Ok(col.clone()),
                [qual, col] => {
                    if !side_matches(update_side, qual) {
                        return Err(ExecError::Message(format!("unknown column '{qual}.{col}'")));
                    }
                    Ok(col.clone())
                }
                _ => Err(ExecError::Message(format!(
                    "unsupported assignment target: {name}"
                ))),
            }
        }
        other => Err(ExecError::Message(format!(
            "unsupported assignment target: {other:?}"
        ))),
    }
}

fn parse_inner_join(
    from: &TableWithJoins,
    session: &Session,
) -> Result<(crate::JoinSide, crate::JoinSide, crate::JoinKey), ExecError> {
    if from.joins.len() != 1 {
        return Err(ExecError::Message(
            "only single INNER JOIN is supported for UPDATE/DELETE".into(),
        ));
    }
    let join = &from.joins[0];
    let on_expr = match &join.join_operator {
        JoinOperator::Inner(JoinConstraint::On(expr)) => expr,
        JoinOperator::CrossJoin => {
            return Err(ExecError::Message(
                "CROSS JOIN is not supported for UPDATE/DELETE".into(),
            ))
        }
        _ => {
            return Err(ExecError::Message(
                "only INNER JOIN is supported for UPDATE/DELETE".into(),
            ))
        }
    };
    let left = crate::join_side_from_factor(&from.relation, session)?;
    let right = crate::join_side_from_factor(&join.relation, session)?;
    let key = crate::parse_join_on_eq(on_expr)?;
    Ok((left, right, key))
}

fn table_meta(session: &Session, table: &str) -> Result<TableMeta, ExecError> {
    session
        .catalog
        .get_table(table)
        .cloned()
        .ok_or_else(|| ExecError::Storage(rusql_storage::StorageError::table_not_found(table)))
}

fn single_pk_column(meta: &TableMeta) -> Result<String, ExecError> {
    let pk: Vec<_> = meta
        .columns
        .iter()
        .filter(|c| c.primary_key)
        .map(|c| c.name.clone())
        .collect();
    if pk.len() != 1 {
        return Err(ExecError::Message(
            "multi-table UPDATE/DELETE requires a single-column primary key".into(),
        ));
    }
    Ok(pk[0].clone())
}

fn row_cell(meta: &TableMeta, row: &Row, name: &str) -> Result<String, ExecError> {
    let idx = meta
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| ExecError::Message(format!("Unknown column '{name}'")))?;
    Ok(row.get(idx).cloned().unwrap_or_default())
}

fn left_row_slice(row: &Row, left_width: usize) -> &[String] {
    &row[..left_width.min(row.len())]
}

fn qualified_join_columns(left: &crate::JoinSide, right: &crate::JoinSide) -> Vec<String> {
    let mut out = Vec::with_capacity(left.columns.len() + right.columns.len());
    for c in &left.columns {
        out.push(format!("{}.{}", left.alias, c));
    }
    for c in &right.columns {
        out.push(format!("{}.{}", right.alias, c));
    }
    out
}

fn eval_join_expr(
    row: &Row,
    left: &crate::JoinSide,
    right: &crate::JoinSide,
    expr: &Expr,
    session: Option<&Session>,
) -> Result<String, ExecError> {
    match expr {
        Expr::CompoundIdentifier(parts) if parts.len() >= 2 => {
            let qual = parts[parts.len() - 2].value.as_str();
            let col = parts.last().unwrap().value.as_str();
            let (side, offset) = if side_matches(left, qual) {
                (left, 0usize)
            } else if side_matches(right, qual) {
                (right, left.columns.len())
            } else {
                return Err(ExecError::Message(format!("unknown column '{qual}.{col}'")));
            };
            let idx = side
                .columns
                .iter()
                .position(|c| c.eq_ignore_ascii_case(col))
                .ok_or_else(|| ExecError::Message(format!("unknown column '{qual}.{col}'")))?;
            Ok(row.get(offset + idx).cloned().unwrap_or_default())
        }
        Expr::Identifier(id) => {
            let idx = left
                .columns
                .iter()
                .position(|c| c.eq_ignore_ascii_case(&id.value))
                .ok_or_else(|| ExecError::Message(format!("unknown column '{}'", id.value)))?;
            Ok(row.get(idx).cloned().unwrap_or_default())
        }
        other => {
            let flat: Vec<String> = left
                .columns
                .iter()
                .chain(right.columns.iter())
                .cloned()
                .collect();
            eval_expr(row, &flat, other, session)
        }
    }
}

fn side_matches(side: &crate::JoinSide, qualifier: &str) -> bool {
    side.alias.eq_ignore_ascii_case(qualifier)
        || side.table_name.eq_ignore_ascii_case(qualifier)
        || table_basename(&side.table_name).eq_ignore_ascii_case(qualifier)
}

fn table_basename(table: &str) -> &str {
    table.rsplit('.').next().unwrap_or(table)
}

fn object_name_to_string(name: &ObjectName) -> String {
    name.0
        .iter()
        .map(|i| i.value.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{heap_executor, plan};
    use rusql_sql::parse;
    use rusql_storage::HeapEngine;
    use sqlparser::ast::Statement;

    fn exec_sql(exec: &mut crate::Executor<HeapEngine>, session: &mut Session, sql: &str) {
        let plans = plan(session, parse(sql).unwrap());
        exec.execute(session, &plans, None).unwrap();
    }

    #[test]
    fn multi_update_inner_join_sets_from_right_table() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE m144_a (id INT PRIMARY KEY, n INT)",
            "CREATE TABLE m144_b (id INT PRIMARY KEY, n INT)",
            "INSERT INTO m144_a VALUES (1, 0), (2, 0)",
            "INSERT INTO m144_b VALUES (1, 10), (3, 99)",
        ] {
            exec_sql(&mut exec, &mut session, sql);
        }
        exec_sql(
            &mut exec,
            &mut session,
            "UPDATE m144_a INNER JOIN m144_b ON m144_a.id = m144_b.id SET m144_a.n = m144_b.n",
        );
        let plans = plan(
            &session,
            parse("SELECT id, n FROM m144_a ORDER BY id").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    &vec![
                        vec!["1".to_string(), "10".to_string()],
                        vec!["2".to_string(), "0".to_string()],
                    ]
                );
            }
            other => panic!("expected rows, got {other:?}"),
        }
    }

    #[test]
    fn multi_delete_inner_join_removes_target_only() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE m144_da (id INT PRIMARY KEY, n INT)",
            "CREATE TABLE m144_db (id INT PRIMARY KEY, n INT)",
            "INSERT INTO m144_da VALUES (1, 1), (2, 2)",
            "INSERT INTO m144_db VALUES (1, 10), (3, 30)",
        ] {
            exec_sql(&mut exec, &mut session, sql);
        }
        exec_sql(
            &mut exec,
            &mut session,
            "DELETE m144_da FROM m144_da INNER JOIN m144_db ON m144_da.id = m144_db.id",
        );
        let plans = plan(
            &session,
            parse("SELECT id FROM m144_da ORDER BY id").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows, &vec![vec!["2".to_string()]]);
            }
            other => panic!("expected one row in da, got {other:?}"),
        }
        let plans = plan(
            &session,
            parse("SELECT id FROM m144_db ORDER BY id").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows, &vec![vec!["1".to_string()], vec!["3".to_string()]]);
            }
            other => panic!("expected db unchanged, got {other:?}"),
        }
    }

    #[test]
    fn multi_update_unknown_column_errors() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        exec_sql(
            &mut exec,
            &mut session,
            "CREATE TABLE m144_x (id INT PRIMARY KEY, n INT)",
        );
        exec_sql(
            &mut exec,
            &mut session,
            "CREATE TABLE m144_y (id INT PRIMARY KEY, n INT)",
        );
        let err = {
            let sql = "UPDATE m144_x INNER JOIN m144_y ON m144_x.id = m144_y.id SET m144_x.bad = 1";
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap_err()
        };
        assert!(
            err.to_string().to_ascii_lowercase().contains("unknown"),
            "expected unknown column, got {err}"
        );
    }

    #[test]
    fn single_table_update_still_works() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        exec_sql(
            &mut exec,
            &mut session,
            "CREATE TABLE m144_s (id INT PRIMARY KEY, n INT)",
        );
        exec_sql(&mut exec, &mut session, "INSERT INTO m144_s VALUES (1, 0)");
        exec_sql(
            &mut exec,
            &mut session,
            "UPDATE m144_s SET n = 5 WHERE id = 1",
        );
        let plans = plan(&session, parse("SELECT n FROM m144_s").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => assert_eq!(rows, &vec![vec!["5".to_string()]]),
            other => panic!("expected rows, got {other:?}"),
        }
    }

    #[test]
    fn update_inner_join_parses() {
        let stmts = parse("UPDATE t1 INNER JOIN t2 ON t1.id=t2.id SET t1.n = t2.n").unwrap();
        assert!(matches!(stmts[0], Statement::Update { .. }));
    }
}

//! MySQL 8.0 table-value constructor: `VALUES ROW(...)`.

use crate::expr::eval_expr;
use crate::ExecError;
use rusql_core::Session;
use rusql_storage::{Row, StorageEngine};
use sqlparser::ast::{Expr, Values};

pub(crate) fn default_values_column_names(width: usize) -> Vec<String> {
    (0..width).map(|i| format!("column_{i}")).collect()
}

pub(crate) fn execute_values<E: StorageEngine>(
    _engine: &mut E,
    session: &Session,
    values: &Values,
) -> Result<(Vec<String>, Vec<Row>), ExecError> {
    if values.rows.is_empty() {
        return Ok((vec![], vec![]));
    }
    let expected_len = values.rows[0].len();
    let columns = default_values_column_names(expected_len);
    let empty_row: Row = vec![];
    let mut out_rows = Vec::with_capacity(values.rows.len());
    for (row_idx, row_exprs) in values.rows.iter().enumerate() {
        if row_exprs.len() != expected_len {
            return Err(ExecError::Mysql {
                code: 1136,
                message: rusql_i18n::messages::sql_values_row_column_count(row_idx + 1),
            });
        }
        let mut row = Vec::with_capacity(expected_len);
        for expr in row_exprs {
            row.push(eval_values_cell(session, expr, &empty_row, &columns)?);
        }
        out_rows.push(row);
    }
    Ok((columns, out_rows))
}

fn eval_values_cell(
    session: &Session,
    expr: &Expr,
    row: &Row,
    columns: &[String],
) -> Result<String, ExecError> {
    eval_expr(row, columns, expr, Some(session))
}

//! Evaluate functional index keys on heap rows (M151).

use rusql_core::TableMeta;
use rusql_sql::parse_generation_expression;
use sqlparser::ast::{BinaryOperator, Expr, UnaryOperator, Value};

use crate::{column_index, Row, StorageError};

pub fn eval_functional_index_key(
    row: &Row,
    table_meta: &TableMeta,
    expr_sql: &str,
) -> Result<String, StorageError> {
    let expr = parse_generation_expression(expr_sql).map_err(|e| {
        StorageError::Message(rusql_i18n::messages::sql_parse_error(&e.to_string()))
    })?;
    eval_expr(row, table_meta, &expr)
}

fn eval_expr(row: &Row, table_meta: &TableMeta, expr: &Expr) -> Result<String, StorageError> {
    match expr {
        Expr::Identifier(id) => {
            let idx = column_index(table_meta, &id.value)?;
            Ok(row.get(idx).cloned().unwrap_or_default())
        }
        Expr::Value(Value::Number(n, _)) => Ok(n.clone()),
        Expr::UnaryOp {
            op: UnaryOperator::Minus,
            expr: inner,
        } => {
            let v = eval_expr(row, table_meta, inner)?;
            let n: i64 = v.parse().map_err(|_| parse_err())?;
            Ok((-n).to_string())
        }
        Expr::UnaryOp {
            op: UnaryOperator::Plus,
            expr: inner,
        } => eval_expr(row, table_meta, inner),
        Expr::BinaryOp { left, op, right } => {
            let l = eval_expr(row, table_meta, left)?;
            let r = eval_expr(row, table_meta, right)?;
            let (a, b): (i64, i64) = (
                l.parse().map_err(|_| parse_err())?,
                r.parse().map_err(|_| parse_err())?,
            );
            let out = match op {
                BinaryOperator::Plus => a + b,
                BinaryOperator::Minus => a - b,
                BinaryOperator::Multiply => a * b,
                BinaryOperator::Divide => {
                    if b == 0 {
                        return Err(parse_err());
                    }
                    a / b
                }
                _ => return Err(parse_err()),
            };
            Ok(out.to_string())
        }
        Expr::Nested(inner) => eval_expr(row, table_meta, inner),
        _ => Err(parse_err()),
    }
}

fn parse_err() -> StorageError {
    StorageError::Message(rusql_i18n::messages::sql_functional_index_unsupported_expr())
}

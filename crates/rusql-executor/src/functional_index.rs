//! Functional index DDL (M151).

use rusql_core::IndexMeta;
use rusql_sql::{
    validate_functional_index_expression, validate_functional_index_sql, PendingFunctionalIndex,
};
use rusql_storage::StorageEngine;
use sqlparser::ast::{Expr, Ident, OrderByExpr};

use crate::ExecError;

pub fn create_pending_functional_index<E: StorageEngine>(
    engine: &mut E,
    table_key: &str,
    pending: &PendingFunctionalIndex,
) -> Result<(), ExecError> {
    let key = validate_functional_index_sql(&pending.expr_sql)
        .map_err(|e| ExecError::Message(e.to_string()))?;
    let meta = IndexMeta {
        name: pending.name.clone(),
        table: table_key.to_string(),
        columns: Vec::new(),
        invisible: false,
        functional_key: Some(key),
        fulltext: false,
    };
    engine.create_index(meta).map_err(Into::into)
}

pub fn index_meta_from_order_columns(
    table: &str,
    index_name: &str,
    order_cols: &[OrderByExpr],
) -> Result<IndexMeta, ExecError> {
    if order_cols.len() == 1 {
        let order_col = &order_cols[0];
        if order_col.asc == Some(false) {
            return Err(ExecError::Message(
                rusql_i18n::messages::sql_functional_index_unsupported_expr(),
            ));
        }
        return match &order_col.expr {
            Expr::Identifier(Ident { value, .. }) => {
                Ok(IndexMeta::single_column(index_name, table, value.clone()))
            }
            other => {
                let key = validate_functional_index_expression(other)
                    .map_err(|e| ExecError::Message(e.to_string()))?;
                Ok(IndexMeta {
                    name: index_name.to_string(),
                    table: table.to_string(),
                    columns: Vec::new(),
                    invisible: false,
                    functional_key: Some(key),
                    fulltext: false,
                })
            }
        };
    }
    let mut columns = Vec::with_capacity(order_cols.len());
    for order_col in order_cols {
        if order_col.asc == Some(false) {
            return Err(ExecError::Message(
                rusql_i18n::messages::sql_functional_index_unsupported_expr(),
            ));
        }
        match &order_col.expr {
            Expr::Identifier(Ident { value, .. }) => columns.push(value.clone()),
            _ => {
                return Err(ExecError::Message(
                    rusql_i18n::messages::sql_functional_index_unsupported_expr(),
                ));
            }
        }
    }
    Ok(IndexMeta::new(index_name, table, columns))
}

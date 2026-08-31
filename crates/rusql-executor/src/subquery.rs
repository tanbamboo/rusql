//! Subquery execution (M42): IN/EXISTS, scalar, derived tables.

use crate::where_filter::{filter_rows, Predicate, WhereFilter};
use crate::{execute_one, scan_table_factor, ExecError, QueryResult};
use rusql_core::Session;
use rusql_planner::Plan;
use rusql_storage::{Row, StorageEngine};
use sqlparser::ast::{Expr, Query, SetExpr, Statement};

pub(crate) fn run_query<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    query: Query,
) -> Result<(Vec<String>, Vec<Row>), ExecError> {
    let result = execute_one(
        engine,
        session,
        &Plan::Statement(Statement::Query(Box::new(query))),
    )?;
    match result {
        QueryResult::Rows { columns, rows } => Ok((columns, rows)),
        QueryResult::Ok { .. } => Ok((vec![], vec![])),
    }
}

pub(crate) fn filter_rows_with_subqueries<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    rows: Vec<Row>,
    columns: &[String],
    filter: &WhereFilter,
) -> Result<Vec<Row>, ExecError> {
    Ok(rows
        .into_iter()
        .filter(|r| row_matches(engine, session, r, columns, filter))
        .collect())
}

fn row_matches<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    row: &Row,
    columns: &[String],
    filter: &WhereFilter,
) -> bool {
    match filter {
        WhereFilter::Pred(pred) => match pred {
            Predicate::InSubquery {
                column,
                subquery,
                negated,
            } => {
                let matched =
                    match eval_in_subquery(engine, session, row, columns, column, subquery) {
                        Ok(v) => v,
                        Err(_) => return false,
                    };
                matched ^ *negated
            }
            Predicate::Exists { subquery, negated } => {
                let matched = match eval_exists(engine, session, row, columns, subquery) {
                    Ok(v) => v,
                    Err(_) => return false,
                };
                matched ^ *negated
            }
            other => crate::where_filter::row_matches_predicate(row, columns, other),
        },
        WhereFilter::And(parts) => parts
            .iter()
            .all(|f| row_matches(engine, session, row, columns, f)),
        WhereFilter::Or(parts) => parts
            .iter()
            .any(|f| row_matches(engine, session, row, columns, f)),
        WhereFilter::Not(inner) => !row_matches(engine, session, row, columns, inner),
    }
}

fn eval_in_subquery<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    row: &Row,
    outer_columns: &[String],
    column: &str,
    subquery: &Query,
) -> Result<bool, ExecError> {
    let col_idx = outer_columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case(column))
        .ok_or_else(|| ExecError::Message(format!("unknown column '{column}'")))?;
    let cell = row.get(col_idx).cloned().unwrap_or_default();

    if is_correlated(subquery, outer_columns) {
        let values = correlated_subquery_values(engine, session, row, outer_columns, subquery)?;
        return Ok(values.iter().any(|v| v == &cell));
    }

    let (_cols, sub_rows) = run_query(engine, session, subquery.clone())?;
    Ok(sub_rows
        .iter()
        .any(|r| r.first().map(|v| v == &cell).unwrap_or(false)))
}

fn eval_exists<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    row: &Row,
    outer_columns: &[String],
    subquery: &Query,
) -> Result<bool, ExecError> {
    if is_correlated(subquery, outer_columns) {
        let matched = correlated_subquery_rows(engine, session, row, outer_columns, subquery)?;
        return Ok(!matched.is_empty());
    }
    let (_cols, sub_rows) = run_query(engine, session, subquery.clone())?;
    Ok(!sub_rows.is_empty())
}

fn correlated_subquery_rows<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    outer_row: &Row,
    outer_columns: &[String],
    subquery: &Query,
) -> Result<Vec<Row>, ExecError> {
    let SetExpr::Select(select) = subquery.body.as_ref() else {
        return Err(ExecError::Message("subquery must be SELECT".into()));
    };
    let from = select
        .from
        .first()
        .ok_or_else(|| ExecError::Message("subquery requires FROM".into()))?;
    let (inner_cols, inner_rows) = scan_table_factor(engine, session, &from.relation)?;
    let Some(where_expr) = select.selection.as_ref() else {
        return Ok(inner_rows);
    };
    let eq_pairs = extract_correlation_equalities(where_expr, outer_columns, &inner_cols)?;
    Ok(inner_rows
        .into_iter()
        .filter(|inner| {
            eq_pairs.iter().all(|(outer_col, inner_col)| {
                let oi = outer_columns
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case(outer_col));
                let ii = inner_cols
                    .iter()
                    .position(|c| c.eq_ignore_ascii_case(inner_col));
                match (oi, ii) {
                    (Some(o), Some(i)) => {
                        outer_row.get(o).unwrap_or(&String::new())
                            == inner.get(i).unwrap_or(&String::new())
                    }
                    _ => false,
                }
            })
        })
        .collect())
}

fn correlated_subquery_values<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    outer_row: &Row,
    outer_columns: &[String],
    subquery: &Query,
) -> Result<Vec<String>, ExecError> {
    let matched = correlated_subquery_rows(engine, session, outer_row, outer_columns, subquery)?;
    Ok(matched.iter().filter_map(|r| r.first().cloned()).collect())
}

fn is_correlated(subquery: &Query, outer_columns: &[String]) -> bool {
    let SetExpr::Select(select) = subquery.body.as_ref() else {
        return false;
    };
    let Some(where_expr) = select.selection.as_ref() else {
        return false;
    };
    references_outer_columns(where_expr, outer_columns)
}

fn references_outer_columns(expr: &Expr, outer_columns: &[String]) -> bool {
    match expr {
        Expr::Identifier(id) => outer_columns
            .iter()
            .any(|c| c.eq_ignore_ascii_case(&id.value)),
        Expr::CompoundIdentifier(parts) => parts.last().is_some_and(|id| {
            outer_columns
                .iter()
                .any(|c| c.eq_ignore_ascii_case(&id.value))
        }),
        Expr::BinaryOp { left, right, .. } => {
            references_outer_columns(left, outer_columns)
                || references_outer_columns(right, outer_columns)
        }
        Expr::UnaryOp { expr, .. } => references_outer_columns(expr, outer_columns),
        Expr::Nested(inner) => references_outer_columns(inner, outer_columns),
        _ => false,
    }
}

fn extract_correlation_equalities(
    expr: &Expr,
    outer_columns: &[String],
    inner_columns: &[String],
) -> Result<Vec<(String, String)>, ExecError> {
    let mut pairs = Vec::new();
    collect_eq_pairs(expr, outer_columns, inner_columns, &mut pairs)?;
    if pairs.is_empty() {
        return Err(ExecError::Message(
            "correlated subquery requires equality join".into(),
        ));
    }
    Ok(pairs)
}

fn collect_eq_pairs(
    expr: &Expr,
    outer_columns: &[String],
    inner_columns: &[String],
    out: &mut Vec<(String, String)>,
) -> Result<(), ExecError> {
    match expr {
        Expr::BinaryOp {
            left,
            op: sqlparser::ast::BinaryOperator::And,
            right,
        } => {
            collect_eq_pairs(left, outer_columns, inner_columns, out)?;
            collect_eq_pairs(right, outer_columns, inner_columns, out)?;
        }
        Expr::BinaryOp {
            left,
            op: sqlparser::ast::BinaryOperator::Eq,
            right,
        } => {
            if let Some(pair) = correlation_pair(left, right, outer_columns, inner_columns) {
                out.push(pair);
            }
        }
        Expr::Nested(inner) => collect_eq_pairs(inner, outer_columns, inner_columns, out)?,
        _ => {}
    }
    Ok(())
}

fn correlation_pair(
    left: &Expr,
    right: &Expr,
    outer_columns: &[String],
    inner_columns: &[String],
) -> Option<(String, String)> {
    if let (Some(o), Some(i)) = (
        expr_in_columns(left, outer_columns),
        expr_in_columns(right, inner_columns),
    ) {
        return Some((o, i));
    }
    if let (Some(i), Some(o)) = (
        expr_in_columns(left, inner_columns),
        expr_in_columns(right, outer_columns),
    ) {
        return Some((o, i));
    }
    None
}

fn expr_in_columns(expr: &Expr, columns: &[String]) -> Option<String> {
    match expr {
        Expr::Identifier(id) if columns.iter().any(|c| c.eq_ignore_ascii_case(&id.value)) => {
            Some(id.value.clone())
        }
        Expr::CompoundIdentifier(parts) => parts.last().and_then(|id| {
            if columns.iter().any(|c| c.eq_ignore_ascii_case(&id.value)) {
                Some(id.value.clone())
            } else {
                None
            }
        }),
        _ => None,
    }
}

pub(crate) fn eval_scalar_subquery<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    query: Query,
) -> Result<String, ExecError> {
    let (_cols, rows) = run_query(engine, session, query)?;
    match rows.len() {
        0 => Ok(String::new()),
        1 => Ok(rows[0].first().cloned().unwrap_or_default()),
        _ => Err(ExecError::Message(
            "scalar subquery returned more than one row".into(),
        )),
    }
}

pub(crate) fn select_from_subquery<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    subquery: Query,
) -> Result<(Vec<String>, Vec<Row>), ExecError> {
    run_query(engine, session, subquery)
}

pub(crate) fn filter_inline_rows<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    rows: Vec<Row>,
    columns: &[String],
    filter: &WhereFilter,
) -> Result<Vec<Row>, ExecError> {
    if filter_has_subquery(filter) {
        filter_rows_with_subqueries(engine, session, rows, columns, filter)
    } else {
        filter_rows(rows, columns, filter)
    }
}

fn filter_has_subquery(filter: &WhereFilter) -> bool {
    match filter {
        WhereFilter::Pred(Predicate::InSubquery { .. } | Predicate::Exists { .. }) => true,
        WhereFilter::And(parts) | WhereFilter::Or(parts) => parts.iter().any(filter_has_subquery),
        WhereFilter::Not(inner) => filter_has_subquery(inner),
        _ => false,
    }
}

pub(crate) fn parse_where_with_subqueries(
    selection: Option<&Expr>,
) -> Result<Option<WhereFilter>, ExecError> {
    parse_where_filter_extended(selection)
}

fn parse_where_filter_extended(selection: Option<&Expr>) -> Result<Option<WhereFilter>, ExecError> {
    let Some(expr) = selection else {
        return Ok(None);
    };
    Ok(Some(parse_or_extended(expr)?))
}

fn parse_or_extended(expr: &Expr) -> Result<WhereFilter, ExecError> {
    if let Expr::BinaryOp {
        left,
        op: sqlparser::ast::BinaryOperator::Or,
        right,
    } = expr
    {
        return Ok(WhereFilter::Or(vec![
            parse_or_extended(left)?,
            parse_or_extended(right)?,
        ]));
    }
    parse_and_extended(expr)
}

fn parse_and_extended(expr: &Expr) -> Result<WhereFilter, ExecError> {
    if let Expr::BinaryOp {
        left,
        op: sqlparser::ast::BinaryOperator::And,
        right,
    } = expr
    {
        return Ok(WhereFilter::And(vec![
            parse_and_extended(left)?,
            parse_and_extended(right)?,
        ]));
    }
    parse_not_extended(expr)
}

fn parse_not_extended(expr: &Expr) -> Result<WhereFilter, ExecError> {
    if let Expr::UnaryOp {
        op: sqlparser::ast::UnaryOperator::Not,
        expr: inner,
    } = expr
    {
        return Ok(WhereFilter::Not(Box::new(parse_not_extended(inner)?)));
    }
    if let Expr::Exists { subquery, negated } = expr {
        return Ok(WhereFilter::Pred(Predicate::Exists {
            subquery: subquery.clone(),
            negated: *negated,
        }));
    }
    Ok(WhereFilter::Pred(parse_predicate_extended(expr)?))
}

fn parse_predicate_extended(expr: &Expr) -> Result<Predicate, ExecError> {
    if let Expr::InSubquery {
        expr: inner,
        subquery,
        negated,
    } = expr
    {
        return Ok(Predicate::InSubquery {
            column: crate::where_filter::expr_column_name_public(inner)?,
            subquery: subquery.clone(),
            negated: *negated,
        });
    }
    crate::where_filter::parse_predicate_public(expr)
}

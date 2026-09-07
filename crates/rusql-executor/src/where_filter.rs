//! WHERE clause parsing and row filtering (M20 + M45).

use crate::ExecError;
use rusql_core::{Collation, DEFAULT_COLLATION};
use rusql_storage::Row;
use sqlparser::ast::{BinaryOperator, Expr, Query, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompareOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Debug, Clone)]
pub(crate) struct LiteralPredicate {
    column: String,
    op: CompareOp,
    value: String,
}

#[derive(Debug, Clone)]
pub(crate) enum Predicate {
    Compare(LiteralPredicate),
    IsNull {
        column: String,
    },
    IsNotNull {
        column: String,
    },
    Like {
        column: String,
        pattern: String,
        negated: bool,
    },
    Between {
        column: String,
        low: String,
        high: String,
        negated: bool,
    },
    In {
        column: String,
        values: Vec<String>,
        negated: bool,
    },
    InSubquery {
        column: String,
        subquery: Box<Query>,
        negated: bool,
    },
    Exists {
        subquery: Box<Query>,
        negated: bool,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum WhereFilter {
    Pred(Predicate),
    And(Vec<WhereFilter>),
    Or(Vec<WhereFilter>),
    Not(Box<WhereFilter>),
}

pub(crate) fn literal_predicate(column: String, op: CompareOp, value: String) -> Predicate {
    Predicate::Compare(LiteralPredicate { column, op, value })
}

pub(crate) fn parse_where_filter(
    selection: Option<&Expr>,
) -> Result<Option<WhereFilter>, ExecError> {
    let Some(expr) = selection else {
        return Ok(None);
    };
    Ok(Some(parse_or(expr)?))
}

pub(crate) fn filter_rows(
    rows: Vec<Row>,
    columns: &[String],
    filter: &WhereFilter,
    column_collations: &[Collation],
) -> Result<Vec<Row>, ExecError> {
    Ok(rows
        .into_iter()
        .filter(|r| row_matches_filter(r, columns, filter, column_collations))
        .collect())
}

fn collation_at(column_collations: &[Collation], idx: usize) -> Collation {
    column_collations
        .get(idx)
        .copied()
        .unwrap_or(DEFAULT_COLLATION)
}

pub(crate) fn extract_eq_predicate(selection: Option<&Expr>) -> Option<(String, String)> {
    let filter = parse_where_filter(selection).ok()??;
    eq_predicate_from_filter(&filter)
}

pub(crate) fn eq_prefix_from_filter(filter: &WhereFilter) -> Vec<(String, String)> {
    match filter {
        WhereFilter::And(parts) => parts
            .iter()
            .filter_map(|part| {
                if let WhereFilter::Pred(Predicate::Compare(lit)) = part {
                    if lit.op == CompareOp::Eq {
                        return Some((lit.column.clone(), lit.value.clone()));
                    }
                }
                None
            })
            .collect(),
        WhereFilter::Pred(Predicate::Compare(lit)) if lit.op == CompareOp::Eq => {
            vec![(lit.column.clone(), lit.value.clone())]
        }
        _ => Vec::new(),
    }
}

pub(crate) fn eq_predicate_from_filter(filter: &WhereFilter) -> Option<(String, String)> {
    let WhereFilter::Pred(Predicate::Compare(pred)) = filter else {
        return None;
    };
    if pred.op != CompareOp::Eq {
        return None;
    }
    Some((pred.column.clone(), pred.value.clone()))
}

pub(crate) fn between_predicate_from_filter(
    filter: &WhereFilter,
) -> Option<(String, String, String)> {
    let WhereFilter::Pred(Predicate::Between {
        column,
        low,
        high,
        negated: false,
    }) = filter
    else {
        return None;
    };
    Some((column.clone(), low.clone(), high.clone()))
}

fn parse_or(expr: &Expr) -> Result<WhereFilter, ExecError> {
    if let Expr::BinaryOp {
        left,
        op: BinaryOperator::Or,
        right,
    } = expr
    {
        return Ok(WhereFilter::Or(vec![parse_or(left)?, parse_or(right)?]));
    }
    parse_and(expr)
}

fn parse_and(expr: &Expr) -> Result<WhereFilter, ExecError> {
    if let Expr::BinaryOp {
        left,
        op: BinaryOperator::And,
        right,
    } = expr
    {
        return Ok(WhereFilter::And(vec![parse_and(left)?, parse_and(right)?]));
    }
    parse_not(expr)
}

fn parse_not(expr: &Expr) -> Result<WhereFilter, ExecError> {
    if let Expr::UnaryOp {
        op: sqlparser::ast::UnaryOperator::Not,
        expr: inner,
    } = expr
    {
        return Ok(WhereFilter::Not(Box::new(parse_not(inner)?)));
    }
    Ok(WhereFilter::Pred(parse_predicate(expr)?))
}

pub(crate) fn parse_predicate_public(expr: &Expr) -> Result<Predicate, ExecError> {
    parse_predicate(expr)
}

pub(crate) fn expr_column_name_public(expr: &Expr) -> Result<String, ExecError> {
    expr_column_name(expr)
}

pub(crate) fn row_matches_predicate(row: &Row, columns: &[String], pred: &Predicate) -> bool {
    row_matches_predicate_impl(row, columns, pred, &[])
}

fn parse_predicate(expr: &Expr) -> Result<Predicate, ExecError> {
    match expr {
        Expr::IsNull(inner) => Ok(Predicate::IsNull {
            column: expr_column_name(inner)?,
        }),
        Expr::IsNotNull(inner) => Ok(Predicate::IsNotNull {
            column: expr_column_name(inner)?,
        }),
        Expr::Like {
            negated,
            expr: inner,
            pattern,
            escape_char: _,
            any: _,
        } => Ok(Predicate::Like {
            column: expr_column_name(inner)?,
            pattern: expr_to_string(pattern)?,
            negated: *negated,
        }),
        Expr::Between {
            expr: inner,
            negated,
            low,
            high,
        } => Ok(Predicate::Between {
            column: expr_column_name(inner)?,
            low: expr_to_string(low)?,
            high: expr_to_string(high)?,
            negated: *negated,
        }),
        Expr::InList {
            expr: inner,
            list,
            negated,
        } => {
            let values = list
                .iter()
                .map(expr_to_string)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Predicate::In {
                column: expr_column_name(inner)?,
                values,
                negated: *negated,
            })
        }
        _ => Ok(Predicate::Compare(parse_literal_predicate(expr)?)),
    }
}

fn parse_literal_predicate(expr: &Expr) -> Result<LiteralPredicate, ExecError> {
    let Expr::BinaryOp { left, op, right } = expr else {
        return Err(ExecError::Message(format!(
            "unsupported WHERE expression: {expr:?}"
        )));
    };
    let column = expr_column_name(left)?;
    let op = match op {
        BinaryOperator::Eq => CompareOp::Eq,
        BinaryOperator::NotEq => CompareOp::NotEq,
        BinaryOperator::Lt => CompareOp::Lt,
        BinaryOperator::LtEq => CompareOp::LtEq,
        BinaryOperator::Gt => CompareOp::Gt,
        BinaryOperator::GtEq => CompareOp::GtEq,
        other => {
            return Err(ExecError::Message(format!(
                "unsupported WHERE operator: {other:?}"
            )))
        }
    };
    let value = expr_to_string(right)?;
    Ok(LiteralPredicate { column, op, value })
}

fn row_matches_filter(
    row: &Row,
    columns: &[String],
    filter: &WhereFilter,
    column_collations: &[Collation],
) -> bool {
    match filter {
        WhereFilter::Pred(pred) => {
            row_matches_predicate_impl(row, columns, pred, column_collations)
        }
        WhereFilter::And(parts) => parts
            .iter()
            .all(|f| row_matches_filter(row, columns, f, column_collations)),
        WhereFilter::Or(parts) => parts
            .iter()
            .any(|f| row_matches_filter(row, columns, f, column_collations)),
        WhereFilter::Not(inner) => !row_matches_filter(row, columns, inner, column_collations),
    }
}

fn row_matches_predicate_impl(
    row: &Row,
    columns: &[String],
    pred: &Predicate,
    column_collations: &[Collation],
) -> bool {
    match pred {
        Predicate::Compare(p) => {
            let col_idx = columns
                .iter()
                .position(|c| c.eq_ignore_ascii_case(&p.column));
            match col_idx {
                Some(i) => row
                    .get(i)
                    .map(|cell| {
                        let collation = collation_at(column_collations, i);
                        compare_values(cell, p.op, &p.value, collation)
                    })
                    .unwrap_or(false),
                None => false,
            }
        }
        Predicate::IsNull { column } => {
            let col_idx = columns.iter().position(|c| c.eq_ignore_ascii_case(column));
            col_idx
                .and_then(|i| row.get(i))
                .map(|cell| is_null_cell(cell.as_str()))
                .unwrap_or(true)
        }
        Predicate::IsNotNull { column } => {
            let col_idx = columns.iter().position(|c| c.eq_ignore_ascii_case(column));
            col_idx
                .and_then(|i| row.get(i))
                .map(|cell| !is_null_cell(cell.as_str()))
                .unwrap_or(false)
        }
        Predicate::Like {
            column,
            pattern,
            negated,
        } => {
            let col_idx = columns.iter().position(|c| c.eq_ignore_ascii_case(column));
            let matched = match col_idx {
                Some(i) => row
                    .get(i)
                    .map(|cell| {
                        let collation = collation_at(column_collations, i);
                        like_match(cell, pattern, collation)
                    })
                    .unwrap_or(false),
                None => false,
            };
            matched ^ *negated
        }
        Predicate::Between {
            column,
            low,
            high,
            negated,
        } => {
            let col_idx = columns.iter().position(|c| c.eq_ignore_ascii_case(column));
            let matched = match col_idx {
                Some(i) => row
                    .get(i)
                    .map(|cell| {
                        let collation = collation_at(column_collations, i);
                        between_inclusive(cell, low, high, collation)
                    })
                    .unwrap_or(false),
                None => false,
            };
            matched ^ *negated
        }
        Predicate::In {
            column,
            values,
            negated,
        } => {
            let col_idx = columns.iter().position(|c| c.eq_ignore_ascii_case(column));
            let matched = match col_idx {
                Some(i) => row
                    .get(i)
                    .map(|cell| {
                        let collation = collation_at(column_collations, i);
                        values.iter().any(|v| collation.eq(cell, v))
                    })
                    .unwrap_or(false),
                None => false,
            };
            matched ^ *negated
        }
        Predicate::InSubquery { .. } | Predicate::Exists { .. } => false,
    }
}

fn compare_values(cell: &str, op: CompareOp, literal: &str, collation: Collation) -> bool {
    if let (Ok(a), Ok(b)) = (cell.parse::<i64>(), literal.parse::<i64>()) {
        return match op {
            CompareOp::Eq => a == b,
            CompareOp::NotEq => a != b,
            CompareOp::Lt => a < b,
            CompareOp::LtEq => a <= b,
            CompareOp::Gt => a > b,
            CompareOp::GtEq => a >= b,
        };
    }
    match op {
        CompareOp::Eq => collation.eq(cell, literal),
        CompareOp::NotEq => !collation.eq(cell, literal),
        CompareOp::Lt => collation.compare(cell, literal).is_lt(),
        CompareOp::LtEq => {
            matches!(
                collation.compare(cell, literal),
                std::cmp::Ordering::Less | std::cmp::Ordering::Equal
            )
        }
        CompareOp::Gt => collation.compare(cell, literal).is_gt(),
        CompareOp::GtEq => {
            matches!(
                collation.compare(cell, literal),
                std::cmp::Ordering::Greater | std::cmp::Ordering::Equal
            )
        }
    }
}

/// SQL LIKE with `%` wildcard only (M45).
fn like_match(cell: &str, pattern: &str, collation: Collation) -> bool {
    if pattern == "%" {
        return true;
    }
    if let Some(middle) = pattern.strip_prefix('%').and_then(|s| s.strip_suffix('%')) {
        return !middle.is_empty() && cell.contains(middle);
    }
    if let Some(prefix) = pattern.strip_suffix('%') {
        return cell.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix('%') {
        return cell.ends_with(suffix);
    }
    cell == pattern || collation.eq(cell, pattern)
}

fn between_inclusive(cell: &str, low: &str, high: &str, collation: Collation) -> bool {
    if let (Ok(c), Ok(lo), Ok(hi)) = (cell.parse::<i64>(), low.parse::<i64>(), high.parse::<i64>())
    {
        return lo <= c && c <= hi;
    }
    if collation.compare(low, high) != std::cmp::Ordering::Greater {
        collation.compare(cell, low).is_ge() && collation.compare(cell, high).is_le()
    } else {
        false
    }
}

fn is_null_cell(cell: &str) -> bool {
    cell.is_empty()
}

fn expr_to_string(expr: &Expr) -> Result<String, ExecError> {
    match expr {
        Expr::Value(Value::Null) => Ok(String::new()),
        Expr::Value(Value::Number(n, _)) => Ok(n.clone()),
        Expr::Value(Value::SingleQuotedString(s)) => Ok(s.clone()),
        other => Err(ExecError::Message(format!("unsupported expr: {other:?}"))),
    }
}

fn expr_column_name(expr: &Expr) -> Result<String, ExecError> {
    match expr {
        Expr::Identifier(id) => Ok(id.value.clone()),
        Expr::CompoundIdentifier(parts) => parts
            .last()
            .map(|id| id.value.clone())
            .ok_or_else(|| ExecError::Message("empty compound identifier".into())),
        other => Err(ExecError::Message(format!(
            "unsupported WHERE column: {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_sql::parse;

    fn filter_sql(sql: &str) -> Vec<Row> {
        let stmt = parse(sql).unwrap().into_iter().next().unwrap();
        let sqlparser::ast::Statement::Query(q) = stmt else {
            panic!("expected query");
        };
        let sqlparser::ast::SetExpr::Select(select) = q.body.as_ref() else {
            panic!("expected select");
        };
        let filter = parse_where_filter(select.selection.as_ref())
            .unwrap()
            .unwrap();
        let columns = vec!["id".into(), "name".into()];
        let rows = vec![
            vec!["1".into(), "alice".into()],
            vec!["2".into(), "bob".into()],
            vec!["3".into(), "carol".into()],
        ];
        filter_rows(rows, &columns, &filter, &[]).unwrap()
    }

    #[test]
    fn collation_case_insensitive_eq() {
        let out = filter_sql("SELECT * FROM t WHERE name = 'ALICE'");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][1], "alice");
    }

    #[test]
    fn or_predicate() {
        let out = filter_sql("SELECT * FROM t WHERE id = 1 OR id = 3");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0][0], "1");
        assert_eq!(out[1][0], "3");
    }

    #[test]
    fn and_or_precedence() {
        let out = filter_sql("SELECT * FROM t WHERE id = 1 OR id = 2 AND name = 'bob'");
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn like_prefix_wildcard() {
        let out = filter_sql("SELECT * FROM t WHERE name LIKE 'a%'");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][1], "alice");
    }

    #[test]
    fn between_inclusive() {
        let out = filter_sql("SELECT * FROM t WHERE id BETWEEN 2 AND 3");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0][0], "2");
        assert_eq!(out[1][0], "3");
    }

    #[test]
    fn in_list() {
        let out = filter_sql("SELECT * FROM t WHERE id IN (1, 3)");
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn not_in_list() {
        let out = filter_sql("SELECT * FROM t WHERE id NOT IN (1, 3)");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][0], "2");
    }

    #[test]
    fn not_like() {
        let out = filter_sql("SELECT * FROM t WHERE name NOT LIKE 'a%'");
        assert_eq!(out.len(), 2);
    }
}

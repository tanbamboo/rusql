//! WHERE clause parsing and row filtering (M20 + M45).

use crate::ExecError;
use rusql_storage::Row;
use sqlparser::ast::{BinaryOperator, Expr, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompareOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Debug, Clone)]
struct LiteralPredicate {
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
}

#[derive(Debug, Clone)]
pub(crate) enum WhereFilter {
    Pred(Predicate),
    And(Vec<WhereFilter>),
    Or(Vec<WhereFilter>),
    Not(Box<WhereFilter>),
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
) -> Result<Vec<Row>, ExecError> {
    Ok(rows
        .into_iter()
        .filter(|r| row_matches_filter(r, columns, filter))
        .collect())
}

pub(crate) fn extract_eq_predicate(selection: Option<&Expr>) -> Option<(String, String)> {
    let filter = parse_where_filter(selection).ok()??;
    eq_predicate_from_filter(&filter)
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

fn row_matches_filter(row: &Row, columns: &[String], filter: &WhereFilter) -> bool {
    match filter {
        WhereFilter::Pred(pred) => row_matches_predicate(row, columns, pred),
        WhereFilter::And(parts) => parts.iter().all(|f| row_matches_filter(row, columns, f)),
        WhereFilter::Or(parts) => parts.iter().any(|f| row_matches_filter(row, columns, f)),
        WhereFilter::Not(inner) => !row_matches_filter(row, columns, inner),
    }
}

fn row_matches_predicate(row: &Row, columns: &[String], pred: &Predicate) -> bool {
    match pred {
        Predicate::Compare(p) => {
            let col_idx = columns
                .iter()
                .position(|c| c.eq_ignore_ascii_case(&p.column));
            col_idx
                .and_then(|i| row.get(i))
                .map(|cell| compare_values(cell, p.op, &p.value))
                .unwrap_or(false)
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
            let matched = col_idx
                .and_then(|i| row.get(i))
                .map(|cell| like_match(cell, pattern))
                .unwrap_or(false);
            matched ^ *negated
        }
        Predicate::Between {
            column,
            low,
            high,
            negated,
        } => {
            let col_idx = columns.iter().position(|c| c.eq_ignore_ascii_case(column));
            let matched = col_idx
                .and_then(|i| row.get(i))
                .map(|cell| between_inclusive(cell, low, high))
                .unwrap_or(false);
            matched ^ *negated
        }
        Predicate::In {
            column,
            values,
            negated,
        } => {
            let col_idx = columns.iter().position(|c| c.eq_ignore_ascii_case(column));
            let matched = col_idx
                .and_then(|i| row.get(i))
                .map(|cell| values.iter().any(|v| cell == v))
                .unwrap_or(false);
            matched ^ *negated
        }
    }
}

fn compare_values(cell: &str, op: CompareOp, literal: &str) -> bool {
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
        CompareOp::Eq => cell == literal,
        CompareOp::NotEq => cell != literal,
        CompareOp::Lt => cell < literal,
        CompareOp::LtEq => cell <= literal,
        CompareOp::Gt => cell > literal,
        CompareOp::GtEq => cell >= literal,
    }
}

/// SQL LIKE with `%` wildcard only (M45).
fn like_match(cell: &str, pattern: &str) -> bool {
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
    cell == pattern
}

fn between_inclusive(cell: &str, low: &str, high: &str) -> bool {
    if let (Ok(c), Ok(lo), Ok(hi)) = (cell.parse::<i64>(), low.parse::<i64>(), high.parse::<i64>())
    {
        return lo <= c && c <= hi;
    }
    low <= cell && cell <= high
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
        filter_rows(rows, &columns, &filter).unwrap()
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

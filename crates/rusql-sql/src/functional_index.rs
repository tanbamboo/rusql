//! MySQL 8.0 functional indexes `INDEX ((expr))` (M151).

use std::cell::RefCell;

use sqlparser::ast::{BinaryOperator, Expr, Function, UnaryOperator, Value};

use crate::generated_expr::parse_generation_expression;
use crate::SqlError;

/// Canonical SQL text for index lookup / WHERE matching (stable spacing).
pub fn canonical_functional_index_sql(expr: &Expr) -> String {
    normalize_expr_display(expr)
}

/// Validate expression for a functional index; returns canonical SQL.
pub fn validate_functional_index_expression(expr: &Expr) -> Result<String, SqlError> {
    validate_expr_node(expr)?;
    Ok(canonical_functional_index_sql(expr))
}

/// Validate from SQL text (CREATE INDEX / inline rewrite).
pub fn validate_functional_index_sql(sql: &str) -> Result<String, SqlError> {
    let expr = parse_generation_expression(sql)?;
    validate_functional_index_expression(&expr)
}

/// Column names referenced by a validated functional index expression.
pub fn functional_index_column_refs(sql: &str) -> Result<Vec<String>, SqlError> {
    let expr = parse_generation_expression(sql)?;
    let mut cols = Vec::new();
    collect_column_refs(&expr, &mut cols);
    cols.sort();
    cols.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    Ok(cols)
}

fn validate_expr_node(expr: &Expr) -> Result<(), SqlError> {
    match expr {
        Expr::Identifier(_) | Expr::Value(Value::Number(_, _)) => Ok(()),
        Expr::UnaryOp {
            op: UnaryOperator::Minus | UnaryOperator::Plus,
            expr: inner,
        } => validate_expr_node(inner),
        Expr::BinaryOp { left, op, right } => {
            if !matches!(
                op,
                BinaryOperator::Plus
                    | BinaryOperator::Minus
                    | BinaryOperator::Multiply
                    | BinaryOperator::Divide
            ) {
                return Err(SqlError::Parse(
                    rusql_i18n::messages::sql_functional_index_unsupported_expr(),
                ));
            }
            validate_expr_node(left)?;
            validate_expr_node(right)
        }
        Expr::Nested(inner) => validate_expr_node(inner),
        Expr::Function(Function { name, .. }) => Err(SqlError::Parse(
            rusql_i18n::messages::sql_functional_index_unknown_function(&name.to_string()),
        )),
        _ => Err(SqlError::Parse(
            rusql_i18n::messages::sql_functional_index_unsupported_expr(),
        )),
    }
}

fn collect_column_refs(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Identifier(id) => out.push(id.value.clone()),
        Expr::UnaryOp { expr: inner, .. } => collect_column_refs(inner, out),
        Expr::BinaryOp { left, right, .. } => {
            collect_column_refs(left, out);
            collect_column_refs(right, out);
        }
        Expr::Nested(inner) => collect_column_refs(inner, out),
        _ => {}
    }
}

fn normalize_expr_display(expr: &Expr) -> String {
    match expr {
        Expr::Identifier(id) => id.value.clone(),
        Expr::Value(Value::Number(n, _)) => n.clone(),
        Expr::UnaryOp {
            op: UnaryOperator::Minus,
            expr: inner,
        } => format!("-{}", normalize_expr_display(inner)),
        Expr::UnaryOp {
            op: UnaryOperator::Plus,
            expr: inner,
        } => normalize_expr_display(inner),
        Expr::BinaryOp { left, op, right } => {
            let l = normalize_expr_display(left);
            let r = normalize_expr_display(right);
            let op_s = match op {
                BinaryOperator::Plus => "+",
                BinaryOperator::Minus => "-",
                BinaryOperator::Multiply => "*",
                BinaryOperator::Divide => "/",
                _ => "+",
            };
            format!("{l} {op_s} {r}")
        }
        Expr::Nested(inner) => normalize_expr_display(inner),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFunctionalIndex {
    pub table: String,
    pub name: String,
    pub expr_sql: String,
}

thread_local! {
    static PENDING_FUNCTIONAL_INDEXES: RefCell<Vec<PendingFunctionalIndex>> =
        const { RefCell::new(Vec::new()) };
}

/// Indexes stripped from the last `CREATE TABLE` rewrite in this thread (single-statement batch).
pub fn take_pending_functional_indexes() -> Vec<PendingFunctionalIndex> {
    PENDING_FUNCTIONAL_INDEXES.with(|p| std::mem::take(&mut *p.borrow_mut()))
}

/// Strip inline `INDEX ((expr))` from `CREATE TABLE`; queue indexes for executor.
pub fn rewrite_create_table_functional_indexes(sql: &str) -> String {
    let trimmed = sql.trim();
    let upper = trimmed.to_ascii_uppercase();
    if !upper.starts_with("CREATE TABLE") {
        return sql.to_string();
    }
    let Some(body_start) = trimmed.find('(') else {
        return sql.to_string();
    };
    let table_part = trimmed[..body_start].trim();
    let Some(table_name) = extract_create_table_name(table_part) else {
        return sql.to_string();
    };

    let mut depth = 0i32;
    let mut body_end = None;
    for (i, ch) in trimmed.char_indices().skip(body_start) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    body_end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(body_end) = body_end else {
        return sql.to_string();
    };

    let body = &trimmed[body_start + 1..body_end];
    let (new_body, extracted) = strip_functional_index_clauses(body);
    if extracted.is_empty() {
        return sql.to_string();
    }

    let suffix = trimmed[body_end + 1..].trim_start();
    PENDING_FUNCTIONAL_INDEXES.with(|p| {
        p.borrow_mut().clear();
        for (idx_name, expr) in &extracted {
            let name = idx_name
                .clone()
                .unwrap_or_else(|| default_functional_index_name(&table_name, expr));
            p.borrow_mut().push(PendingFunctionalIndex {
                table: table_name.clone(),
                name,
                expr_sql: expr.clone(),
            });
        }
    });
    format!("CREATE TABLE {table_name} ({new_body}){suffix}")
}

fn strip_functional_index_clauses(body: &str) -> (String, Vec<(Option<String>, String)>) {
    let mut extracted = Vec::new();
    let mut out = String::new();
    let mut i = 0usize;
    let bytes = body.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b',' {
            let rest = &body[i + 1..];
            if let Some((name, expr, consumed)) = try_consume_functional_index_segment(rest) {
                extracted.push((name, expr));
                i += 1 + consumed;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    (out, extracted)
}

fn extract_create_table_name(table_part: &str) -> Option<String> {
    let rest = table_part
        .strip_prefix("CREATE TABLE")
        .or_else(|| table_part.strip_prefix("create table"))?
        .trim();
    let token: String = rest
        .chars()
        .take_while(|c| *c != '(' && !c.is_whitespace())
        .collect();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn default_functional_index_name(table: &str, expr: &str) -> String {
    let base: String = expr
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("idx_{table}_{base}")
}

fn try_consume_functional_index_segment(segment: &str) -> Option<(Option<String>, String, usize)> {
    let bytes = segment.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    let upper = segment[pos..].to_ascii_uppercase();
    if upper.starts_with("INDEX ") {
        pos += 6;
    } else if upper.starts_with("KEY ") {
        pos += 4;
    } else {
        return None;
    }
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    let index_name = if segment[pos..].starts_with("((") {
        None
    } else {
        let name_end = segment[pos..]
            .find(|c: char| c.is_whitespace() || c == '(')
            .unwrap_or(segment.len() - pos);
        let name = segment[pos..pos + name_end].trim().to_string();
        pos += name_end;
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        Some(name)
    };
    if !segment[pos..].starts_with("((") {
        return None;
    }
    pos += 2;
    let close = segment[pos..].find("))")?;
    let expr = segment[pos..pos + close].trim();
    if expr.is_empty() {
        return None;
    }
    pos += close + 2;
    Some((index_name.filter(|n| !n.is_empty()), expr.to_string(), pos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use sqlparser::ast::Statement;

    #[test]
    fn validate_rejects_unknown_function() {
        let err = validate_functional_index_sql("ABS(n)").unwrap_err();
        assert!(err.to_string().contains("ABS") || !err.to_string().is_empty());
    }

    #[test]
    fn validate_accepts_arithmetic() {
        let canon = validate_functional_index_sql("n+1").unwrap();
        assert_eq!(canon, "n + 1");
    }

    #[test]
    fn rewrite_create_table_queues_functional_index() {
        let sql = "CREATE TABLE t (id INT PRIMARY KEY, n INT, INDEX ((n+1)))";
        let rewritten = rewrite_create_table_functional_indexes(sql);
        assert!(rewritten.contains("CREATE TABLE t (id INT PRIMARY KEY, n INT)"));
        assert!(!rewritten.contains("CREATE INDEX"));
        let pending = take_pending_functional_indexes();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].expr_sql, "n+1");
        let stmts = parse(&rewritten).unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(stmts[0], Statement::CreateTable(_)));
        let _ = take_pending_functional_indexes();
    }
}

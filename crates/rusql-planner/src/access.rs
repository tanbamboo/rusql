//! Index-aware access-path selection for simple single-table SELECT (M49).

use sqlparser::ast::{BinaryOperator, Expr, Select, SetExpr, Value};

/// MySQL EXPLAIN `type` column values (subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessType {
    All,
    Const,
    Ref,
    Range,
}

impl AccessType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Const => "const",
            Self::Ref => "ref",
            Self::Range => "range",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexInfo {
    pub name: String,
    pub column: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainPlanRow {
    pub id: u32,
    pub select_type: String,
    pub table: String,
    pub access_type: AccessType,
    pub key: Option<String>,
    pub rows: u64,
    pub extra: String,
}

pub fn explain_simple_select(
    table: &str,
    selection: Option<&Expr>,
    indexes: &[IndexInfo],
    row_count: u64,
) -> ExplainPlanRow {
    let (access_type, key, estimated_rows) = choose_access(selection, indexes, row_count.max(1));
    ExplainPlanRow {
        id: 1,
        select_type: "SIMPLE".into(),
        table: table.to_string(),
        access_type,
        key,
        rows: estimated_rows,
        extra: String::new(),
    }
}

fn choose_access(
    selection: Option<&Expr>,
    indexes: &[IndexInfo],
    row_count: u64,
) -> (AccessType, Option<String>, u64) {
    let Some(expr) = selection else {
        return (AccessType::All, None, row_count);
    };
    if let Some((column, value)) = parse_eq(expr) {
        if let Some(idx) = find_index(indexes, &column) {
            let access = if idx.name.eq_ignore_ascii_case("PRIMARY") && value_is_const(&value) {
                AccessType::Const
            } else {
                AccessType::Ref
            };
            return (access, Some(idx.name.clone()), 1);
        }
    }
    if let Some((column, _low, _high)) = parse_between(expr) {
        if let Some(idx) = find_index(indexes, &column) {
            let est = (row_count / 10).max(1);
            return (AccessType::Range, Some(idx.name.clone()), est);
        }
    }
    (AccessType::All, None, row_count)
}

fn find_index<'a>(indexes: &'a [IndexInfo], column: &str) -> Option<&'a IndexInfo> {
    indexes
        .iter()
        .find(|i| i.column.eq_ignore_ascii_case(column))
}

fn value_is_const(value: &str) -> bool {
    !value.is_empty()
}

fn parse_eq(expr: &Expr) -> Option<(String, String)> {
    let Expr::BinaryOp { left, op, right } = expr else {
        return None;
    };
    if !matches!(op, BinaryOperator::Eq) {
        return None;
    }
    let column = expr_column(left)?;
    let value = expr_literal(right)?;
    Some((column, value))
}

fn parse_between(expr: &Expr) -> Option<(String, String, String)> {
    let Expr::Between {
        expr,
        low,
        high,
        negated,
        ..
    } = expr
    else {
        return None;
    };
    if *negated {
        return None;
    }
    let column = expr_column(expr)?;
    let low_v = expr_literal(low)?;
    let high_v = expr_literal(high)?;
    Some((column, low_v, high_v))
}

fn expr_column(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(id) => Some(id.value.clone()),
        Expr::CompoundIdentifier(parts) => parts.last().map(|id| id.value.clone()),
        _ => None,
    }
}

fn expr_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Value(Value::Number(n, _)) => Some(n.clone()),
        Expr::Value(Value::SingleQuotedString(s)) => Some(s.clone()),
        Expr::Value(Value::Null) => Some(String::new()),
        _ => None,
    }
}

pub fn table_name_from_select(select: &Select) -> Option<String> {
    let from = select.from.first()?;
    match &from.relation {
        sqlparser::ast::TableFactor::Table { name, .. } => Some(name.0.last()?.value.clone()),
        _ => None,
    }
}

pub fn explain_query_statement(
    statement: &sqlparser::ast::Statement,
    indexes: &[IndexInfo],
    row_count: u64,
) -> Option<ExplainPlanRow> {
    let inner = match statement {
        sqlparser::ast::Statement::Explain { statement, .. } => statement.as_ref(),
        other => other,
    };
    let sqlparser::ast::Statement::Query(query) = inner else {
        return None;
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        return None;
    };
    let table = table_name_from_select(select)?;
    Some(explain_simple_select(
        &table,
        select.selection.as_ref(),
        indexes,
        row_count,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_sql::parse;

    fn indexes() -> Vec<IndexInfo> {
        vec![
            IndexInfo {
                name: "PRIMARY".into(),
                column: "id".into(),
            },
            IndexInfo {
                name: "idx_k".into(),
                column: "k".into(),
            },
        ]
    }

    #[test]
    fn eq_on_index_uses_ref() {
        let stmts = parse("SELECT * FROM t WHERE k = 42").unwrap();
        let row = explain_query_statement(&stmts[0], &indexes(), 10_000).unwrap();
        assert_eq!(row.access_type, AccessType::Ref);
        assert_eq!(row.key.as_deref(), Some("idx_k"));
        assert_eq!(row.rows, 1);
    }

    #[test]
    fn eq_on_pk_uses_const() {
        let stmts = parse("SELECT * FROM t WHERE id = 1").unwrap();
        let row = explain_query_statement(&stmts[0], &indexes(), 10_000).unwrap();
        assert_eq!(row.access_type, AccessType::Const);
        assert_eq!(row.key.as_deref(), Some("PRIMARY"));
    }

    #[test]
    fn between_uses_range() {
        let stmts = parse("SELECT * FROM t WHERE k BETWEEN 1 AND 100").unwrap();
        let row = explain_query_statement(&stmts[0], &indexes(), 10_000).unwrap();
        assert_eq!(row.access_type, AccessType::Range);
        assert_eq!(row.key.as_deref(), Some("idx_k"));
    }

    #[test]
    fn no_predicate_full_scan() {
        let stmts = parse("SELECT * FROM t").unwrap();
        let row = explain_query_statement(&stmts[0], &indexes(), 10_000).unwrap();
        assert_eq!(row.access_type, AccessType::All);
        assert!(row.key.is_none());
        assert_eq!(row.rows, 10_000);
    }

    #[test]
    fn large_table_plan_prefers_index() {
        use rusql_core::{ColumnDef, TableMeta};
        use rusql_storage::{HeapEngine, StorageEngine};
        let mut eng = HeapEngine::new();
        eng.create_table(TableMeta {
            name: "big".into(),
            schema: "rusql".into(),
            columns: vec![ColumnDef::new("id", "INT"), ColumnDef::new("k", "INT")],
            auto_increment_next: None,
            ..Default::default()
        })
        .unwrap();
        eng.create_index(rusql_core::IndexMeta {
            name: "idx_k".into(),
            table: "big".into(),
            column: "k".into(),
        })
        .unwrap();
        for i in 0..10_000 {
            eng.insert("big", vec![i.to_string(), (i % 100).to_string()])
                .unwrap();
        }
        let indexes = vec![IndexInfo {
            name: "idx_k".into(),
            column: "k".into(),
        }];
        let stmts = rusql_sql::parse("SELECT * FROM big WHERE k = 42").unwrap();
        let row = explain_query_statement(&stmts[0], &indexes, 10_000).unwrap();
        assert_eq!(row.access_type, AccessType::Ref);
        assert_eq!(row.key.as_deref(), Some("idx_k"));
    }
}

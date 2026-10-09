//! WITH / CTE rewrite: M69 non-recursive inlining plus M127 recursive UNION ALL.

use crate::{
    execute_set_expr, inline_nonrecursive_ctes, object_name_to_string, substitute_ctes_in_query,
    substitute_ctes_in_set_expr, ExecError,
};
use rusql_core::{PrivilegeStore, Session};
use rusql_storage::{Row, StorageEngine};
use sqlparser::ast::{
    Cte, Expr, ObjectName, Query, SetExpr, SetOperator, SetQuantifier, Statement, TableFactor,
    TableWithJoins,
};

/// MySQL 8.0 default `cte_max_recursion_depth` (not SET-able in this slice).
pub(crate) const CTE_MAX_RECURSION_DEPTH: u32 = 1000;

/// MySQL `ER_CTE_MAX_RECURSION_DEPTH`.
const ER_CTE_MAX_RECURSION_DEPTH: u16 = 3636;

pub(crate) fn rewrite_query_with_clause<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    query: &Query,
    privileges: &PrivilegeStore,
) -> Result<Query, ExecError> {
    let Some(with) = &query.with else {
        return Ok(query.clone());
    };
    if !with.recursive {
        return inline_nonrecursive_ctes(query);
    }
    let mut bound: Vec<(String, Query)> = Vec::new();
    for cte in &with.cte_tables {
        let name = cte.alias.name.value.clone();
        if query_references_cte(cte.query.as_ref(), &name) {
            let materialized = execute_recursive_cte(engine, session, cte, &bound, privileges)?;
            bound.push((name, materialized));
        } else {
            let mut inner = (*cte.query).clone();
            substitute_ctes_in_query(&mut inner, &bound)?;
            bound.push((name, inner));
        }
    }
    let mut out = query.clone();
    out.with = None;
    substitute_ctes_in_query(&mut out, &bound)?;
    Ok(out)
}

fn execute_recursive_cte<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    cte: &Cte,
    bound: &[(String, Query)],
    privileges: &PrivilegeStore,
) -> Result<Query, ExecError> {
    let name = cte.alias.name.value.clone();
    let mut body = (*cte.query).clone();
    substitute_ctes_in_query(&mut body, bound)?;
    let (union_all, anchors, recursive_members) = partition_union(body.body.as_ref(), &name)?;
    if anchors.is_empty() || recursive_members.is_empty() {
        return Err(ExecError::Message(
            rusql_i18n::messages::sql_with_recursive_unsupported(),
        ));
    }

    let mut columns: Vec<String> = Vec::new();
    let mut result: Vec<Row> = Vec::new();
    for anchor in &anchors {
        let (cols, rows) = execute_set_expr(engine, session, anchor, privileges)?;
        if columns.is_empty() {
            columns = overlay_column_names(&cols, cte, anchor);
        } else if cols.len() != columns.len() {
            return Err(ExecError::Message(
                rusql_i18n::messages::sql_with_recursive_unsupported(),
            ));
        }
        result.extend(rows);
    }
    if !union_all {
        result = crate::dedupe_rows(result);
    }
    if !cte.alias.columns.is_empty() {
        if cte.alias.columns.len() != columns.len() {
            return Err(ExecError::Message(
                rusql_i18n::messages::sql_with_recursive_unsupported(),
            ));
        }
        columns = cte
            .alias
            .columns
            .iter()
            .map(|c| c.name.value.clone())
            .collect();
    }

    let mut working = result.clone();
    let mut depth = 0_u32;
    loop {
        if working.is_empty() {
            break;
        }
        depth += 1;
        if depth > CTE_MAX_RECURSION_DEPTH {
            return Err(ExecError::Mysql {
                code: ER_CTE_MAX_RECURSION_DEPTH,
                message: rusql_i18n::messages::sql_cte_max_recursion_depth(CTE_MAX_RECURSION_DEPTH),
            });
        }
        let working_query = rows_to_query(&columns, &working)?;
        let cte_bound = [(name.clone(), working_query)];
        let mut new_rows: Vec<Row> = Vec::new();
        for member in &recursive_members {
            let mut member = member.clone();
            substitute_ctes_in_set_expr(&mut member, &cte_bound)?;
            let (cols, rows) = execute_set_expr(engine, session, &member, privileges)?;
            if cols.len() != columns.len() {
                return Err(ExecError::Message(
                    rusql_i18n::messages::sql_with_recursive_unsupported(),
                ));
            }
            new_rows.extend(rows);
        }
        if !union_all {
            let mut seen: std::collections::HashSet<String> =
                result.iter().map(|r| r.join("\x1f")).collect();
            new_rows.retain(|r| seen.insert(r.join("\x1f")));
        }
        working = new_rows.clone();
        result.extend(new_rows);
    }
    rows_to_query(&columns, &result)
}

fn overlay_column_names(executed: &[String], cte: &Cte, anchor: &SetExpr) -> Vec<String> {
    if !cte.alias.columns.is_empty() {
        return cte
            .alias
            .columns
            .iter()
            .map(|c| c.name.value.clone())
            .collect();
    }
    if let Some(names) = first_select_aliases(anchor) {
        if names.len() == executed.len() {
            return names;
        }
    }
    executed.to_vec()
}

fn first_select_aliases(expr: &SetExpr) -> Option<Vec<String>> {
    match expr {
        SetExpr::Select(select) => {
            let mut names = Vec::with_capacity(select.projection.len());
            for item in &select.projection {
                match item {
                    sqlparser::ast::SelectItem::ExprWithAlias { alias, .. } => {
                        names.push(alias.value.clone());
                    }
                    sqlparser::ast::SelectItem::UnnamedExpr(expr) => {
                        names.push(crate::expr::expr_output_name(expr, None).ok()?);
                    }
                    _ => return None,
                }
            }
            Some(names)
        }
        SetExpr::Query(q) => first_select_aliases(q.body.as_ref()),
        SetExpr::SetOperation { left, .. } => first_select_aliases(left),
        _ => None,
    }
}

fn partition_union(
    expr: &SetExpr,
    cte_name: &str,
) -> Result<(bool, Vec<SetExpr>, Vec<SetExpr>), ExecError> {
    let parts = flatten_union_parts(expr)?;
    let mut union_all: Option<bool> = None;
    let mut anchors = Vec::new();
    let mut recursive_members = Vec::new();
    let mut saw_recursive = false;
    for (quantifier, part) in parts {
        let is_all = match quantifier {
            None => None,
            Some(SetQuantifier::All) => Some(true),
            Some(SetQuantifier::None) | Some(SetQuantifier::Distinct) => Some(false),
            Some(_) => {
                return Err(ExecError::Message(
                    rusql_i18n::messages::sql_with_recursive_unsupported(),
                ));
            }
        };
        if let Some(is_all) = is_all {
            match union_all {
                None => union_all = Some(is_all),
                Some(prev) if prev != is_all => {
                    return Err(ExecError::Message(
                        rusql_i18n::messages::sql_with_recursive_unsupported(),
                    ));
                }
                Some(_) => {}
            }
        }
        if set_expr_references_cte(part, cte_name) {
            saw_recursive = true;
            recursive_members.push(part.clone());
        } else {
            if saw_recursive {
                return Err(ExecError::Message(
                    rusql_i18n::messages::sql_with_recursive_unsupported(),
                ));
            }
            anchors.push(part.clone());
        }
    }
    Ok((union_all.unwrap_or(true), anchors, recursive_members))
}

/// Flatten a UNION tree into left-to-right parts. Non-UNION bodies are a single part.
fn flatten_union_parts(
    expr: &SetExpr,
) -> Result<Vec<(Option<SetQuantifier>, &SetExpr)>, ExecError> {
    match expr {
        SetExpr::SetOperation {
            op,
            set_quantifier,
            left,
            right,
        } => {
            if *op != SetOperator::Union {
                return Err(ExecError::Message(
                    rusql_i18n::messages::sql_with_recursive_unsupported(),
                ));
            }
            let mut parts = flatten_union_parts(left)?;
            parts.extend(flatten_union_parts(right)?);
            for (q, _) in parts.iter_mut() {
                if q.is_none() {
                    *q = Some(*set_quantifier);
                }
            }
            Ok(parts)
        }
        SetExpr::Query(q) if q.with.is_none() => flatten_union_parts(q.body.as_ref()),
        other => Ok(vec![(None, other)]),
    }
}

fn query_references_cte(query: &Query, cte_name: &str) -> bool {
    if let Some(with) = &query.with {
        if with
            .cte_tables
            .iter()
            .any(|cte| query_references_cte(cte.query.as_ref(), cte_name))
        {
            return true;
        }
    }
    set_expr_references_cte(query.body.as_ref(), cte_name)
}

fn set_expr_references_cte(expr: &SetExpr, cte_name: &str) -> bool {
    match expr {
        SetExpr::Select(select) => {
            select
                .from
                .iter()
                .any(|from| table_with_joins_refs(from, cte_name))
                || expr_references_cte(select.selection.as_ref(), cte_name)
                || select
                    .projection
                    .iter()
                    .any(|item| select_item_references_cte(item, cte_name))
        }
        SetExpr::Query(q) => query_references_cte(q, cte_name),
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_references_cte(left, cte_name) || set_expr_references_cte(right, cte_name)
        }
        SetExpr::Insert(stmt) | SetExpr::Update(stmt) => statement_references_cte(stmt, cte_name),
        _ => false,
    }
}

fn statement_references_cte(stmt: &Statement, cte_name: &str) -> bool {
    if let Statement::Query(q) = stmt {
        return query_references_cte(q, cte_name);
    }
    false
}

fn table_with_joins_refs(from: &TableWithJoins, cte_name: &str) -> bool {
    factor_references_cte(&from.relation, cte_name)
        || from
            .joins
            .iter()
            .any(|join| factor_references_cte(&join.relation, cte_name))
}

fn factor_references_cte(factor: &TableFactor, cte_name: &str) -> bool {
    match factor {
        TableFactor::Table { name, .. } => object_is_cte(name, cte_name),
        TableFactor::Derived { subquery, .. } => query_references_cte(subquery, cte_name),
        TableFactor::NestedJoin {
            table_with_joins, ..
        } => table_with_joins_refs(table_with_joins, cte_name),
        _ => false,
    }
}

fn object_is_cte(name: &ObjectName, cte_name: &str) -> bool {
    object_name_to_string(name).eq_ignore_ascii_case(cte_name)
}

fn select_item_references_cte(item: &sqlparser::ast::SelectItem, cte_name: &str) -> bool {
    match item {
        sqlparser::ast::SelectItem::UnnamedExpr(expr)
        | sqlparser::ast::SelectItem::ExprWithAlias { expr, .. } => {
            expr_references_cte(Some(expr), cte_name)
        }
        _ => false,
    }
}

fn expr_references_cte(expr: Option<&Expr>, cte_name: &str) -> bool {
    let Some(expr) = expr else {
        return false;
    };
    match expr {
        Expr::Subquery(q) | Expr::Exists { subquery: q, .. } => query_references_cte(q, cte_name),
        Expr::InSubquery { expr, subquery, .. } => {
            expr_references_cte(Some(expr), cte_name) || query_references_cte(subquery, cte_name)
        }
        Expr::BinaryOp { left, right, .. } => {
            expr_references_cte(Some(left), cte_name) || expr_references_cte(Some(right), cte_name)
        }
        Expr::UnaryOp { expr, .. } => expr_references_cte(Some(expr), cte_name),
        Expr::Nested(inner) => expr_references_cte(Some(inner), cte_name),
        Expr::Case {
            operand,
            conditions,
            results,
            else_result,
        } => {
            expr_references_cte(operand.as_deref(), cte_name)
                || conditions
                    .iter()
                    .any(|c| expr_references_cte(Some(c), cte_name))
                || results
                    .iter()
                    .any(|r| expr_references_cte(Some(r), cte_name))
                || expr_references_cte(else_result.as_deref(), cte_name)
        }
        _ => false,
    }
}

fn rows_to_query(columns: &[String], rows: &[Row]) -> Result<Query, ExecError> {
    if columns.is_empty() {
        return parse_query("SELECT 1 WHERE 0");
    }
    if rows.is_empty() {
        let aliases = columns
            .iter()
            .map(|c| format!("NULL AS {}", quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        return parse_query(&format!(
            "SELECT * FROM (SELECT {aliases}) AS _rusql_cte_empty WHERE 0"
        ));
    }
    let mut parts = Vec::with_capacity(rows.len());
    for row in rows {
        if row.len() != columns.len() {
            return Err(ExecError::Message(
                rusql_i18n::messages::sql_with_recursive_unsupported(),
            ));
        }
        let list = columns
            .iter()
            .zip(row.iter())
            .map(|(c, v)| format!("{} AS {}", sql_cell_literal(v), quote_ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("SELECT {list}"));
    }
    parse_query(&parts.join(" UNION ALL "))
}

fn parse_query(sql: &str) -> Result<Query, ExecError> {
    let stmts = rusql_sql::parse(sql).map_err(|e| ExecError::Message(e.to_string()))?;
    match stmts.into_iter().next() {
        Some(Statement::Query(q)) => Ok(*q),
        _ => Err(ExecError::Message(
            rusql_i18n::messages::sql_with_recursive_unsupported(),
        )),
    }
}

fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

fn sql_cell_literal(value: &str) -> String {
    if value.is_empty() {
        return "NULL".to_string();
    }
    if is_sql_number(value) {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "''"))
}

fn is_sql_number(value: &str) -> bool {
    let mut chars = value.chars().peekable();
    if chars.peek() == Some(&'-') {
        chars.next();
    }
    let rest: String = chars.collect();
    if rest.is_empty() {
        return false;
    }
    let mut saw_digit = false;
    let mut saw_dot = false;
    for c in rest.chars() {
        if c.is_ascii_digit() {
            saw_digit = true;
        } else if c == '.' && !saw_dot {
            saw_dot = true;
        } else {
            return false;
        }
    }
    saw_digit
}

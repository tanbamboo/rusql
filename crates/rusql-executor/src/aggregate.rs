//! GROUP BY, HAVING, and aggregate functions (M43).

use crate::where_filter::{filter_rows, literal_predicate, CompareOp, Predicate, WhereFilter};
use crate::ExecError;
use rusql_storage::Row;
use sqlparser::ast::{
    Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, GroupByExpr, ObjectName,
    Select, SelectItem,
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
enum AggFn {
    CountStar,
    CountCol(String),
    Sum(String),
    Min(String),
    Max(String),
    Avg(String),
}

#[derive(Debug, Clone)]
enum ProjItem {
    GroupCol { col: String, alias: Option<String> },
    Aggregate { func: AggFn, alias: Option<String> },
}

pub fn select_has_group_by(select: &Select) -> bool {
    match &select.group_by {
        GroupByExpr::Expressions(exprs, _) => !exprs.is_empty(),
        GroupByExpr::All(_) => false,
    }
}

pub fn execute_group_by(
    select: &Select,
    table_columns: &[String],
    mut rows: Vec<Row>,
) -> Result<(Vec<String>, Vec<Row>), ExecError> {
    let group_exprs = match &select.group_by {
        GroupByExpr::Expressions(exprs, _) if !exprs.is_empty() => exprs.clone(),
        GroupByExpr::All(_) => {
            return Err(ExecError::Message("GROUP BY ALL not supported".into()));
        }
        _ => return Err(ExecError::Message("GROUP BY required".into())),
    };

    let group_indices: Vec<usize> = group_exprs
        .iter()
        .map(|e| column_index(table_columns, &expr_column_name(e)?))
        .collect::<Result<_, _>>()?;

    let proj = parse_projection(select, table_columns, &group_exprs)?;
    validate_only_full_group_by(&proj, &group_exprs)?;

    let mut groups: HashMap<String, Vec<Row>> = HashMap::new();
    for row in rows.drain(..) {
        let key = group_key(&row, &group_indices);
        groups.entry(key).or_default().push(row);
    }

    let out_columns: Vec<String> = proj
        .iter()
        .map(|p| match p {
            ProjItem::GroupCol { alias, col } => alias.clone().unwrap_or_else(|| col.clone()),
            ProjItem::Aggregate { func, alias } => {
                alias.clone().unwrap_or_else(|| agg_display_name(func))
            }
        })
        .collect();

    let mut out_rows = Vec::with_capacity(groups.len());
    for (_key, group_rows) in groups {
        let mut out_row = Vec::with_capacity(proj.len());
        for item in &proj {
            match item {
                ProjItem::GroupCol { col, .. } => {
                    let idx = column_index(table_columns, col)?;
                    out_row.push(group_rows[0][idx].clone());
                }
                ProjItem::Aggregate { func, .. } => {
                    out_row.push(compute_agg(func, &group_rows, table_columns)?);
                }
            }
        }
        out_rows.push(out_row);
    }

    if let Some(having) = select.having.as_ref() {
        let filter = parse_having(having, &out_columns)?;
        out_rows = filter_rows(out_rows, &out_columns, &filter)?;
    }

    Ok((out_columns, out_rows))
}

fn parse_projection(
    select: &Select,
    table_columns: &[String],
    group_exprs: &[Expr],
) -> Result<Vec<ProjItem>, ExecError> {
    let mut items = Vec::with_capacity(select.projection.len());
    for sel_item in &select.projection {
        let (expr, alias) = match sel_item {
            SelectItem::UnnamedExpr(expr) => (expr, None),
            SelectItem::ExprWithAlias { expr, alias } => (expr, Some(alias.value.clone())),
            other => {
                return Err(ExecError::Message(format!(
                    "unsupported SELECT item in GROUP BY query: {other:?}"
                )))
            }
        };
        if let Some(func) = parse_agg_fn(expr)? {
            items.push(ProjItem::Aggregate { func, alias });
        } else {
            let col = expr_column_name(expr)?;
            if !group_exprs
                .iter()
                .any(|g| expr_column_name(g).ok().as_deref() == Some(&col))
            {
                return Err(ExecError::Message(format!(
                    "column '{col}' must appear in GROUP BY clause"
                )));
            }
            let _ = column_index(table_columns, &col)?;
            items.push(ProjItem::GroupCol { col, alias });
        }
    }
    Ok(items)
}

fn validate_only_full_group_by(proj: &[ProjItem], group_exprs: &[Expr]) -> Result<(), ExecError> {
    let group_names: Vec<String> = group_exprs
        .iter()
        .map(expr_column_name)
        .collect::<Result<_, _>>()?;
    for item in proj {
        if let ProjItem::GroupCol { col, .. } = item {
            if !group_names.iter().any(|g| g.eq_ignore_ascii_case(col)) {
                return Err(ExecError::Message(format!(
                    "column '{col}' must appear in GROUP BY clause"
                )));
            }
        }
    }
    Ok(())
}

fn parse_agg_fn(expr: &Expr) -> Result<Option<AggFn>, ExecError> {
    let Expr::Function(func) = expr else {
        return Ok(None);
    };
    let name = object_name_last(&func.name)?.to_ascii_uppercase();
    match name.as_str() {
        "COUNT" => {
            let arg = single_function_arg(func)?;
            Ok(Some(match arg {
                FunctionArgExpr::Wildcard => AggFn::CountStar,
                FunctionArgExpr::Expr(inner) => AggFn::CountCol(expr_column_name(&inner)?),
                other => {
                    return Err(ExecError::Message(format!(
                        "unsupported COUNT argument: {other:?}"
                    )))
                }
            }))
        }
        "SUM" => Ok(Some(AggFn::Sum(expr_column_name(&function_arg_expr(
            func,
        )?)?))),
        "MIN" => Ok(Some(AggFn::Min(expr_column_name(&function_arg_expr(
            func,
        )?)?))),
        "MAX" => Ok(Some(AggFn::Max(expr_column_name(&function_arg_expr(
            func,
        )?)?))),
        "AVG" => Ok(Some(AggFn::Avg(expr_column_name(&function_arg_expr(
            func,
        )?)?))),
        other => Err(ExecError::Message(format!(
            "unsupported aggregate function: {other}"
        ))),
    }
}

fn function_arg_expr(func: &Function) -> Result<Expr, ExecError> {
    match &func.args {
        FunctionArguments::List(list) if list.args.len() == 1 => match &list.args[0] {
            FunctionArg::Unnamed(arg) | FunctionArg::Named { arg, .. } => match arg {
                FunctionArgExpr::Expr(expr) => Ok(expr.clone()),
                other => Err(ExecError::Message(format!(
                    "unsupported function argument: {other:?}"
                ))),
            },
            other => Err(ExecError::Message(format!(
                "unsupported function argument: {other:?}"
            ))),
        },
        other => Err(ExecError::Message(format!(
            "unsupported aggregate arguments: {other:?}"
        ))),
    }
}

fn single_function_arg(func: &Function) -> Result<FunctionArgExpr, ExecError> {
    match &func.args {
        FunctionArguments::List(list) if list.args.len() == 1 => match &list.args[0] {
            FunctionArg::Unnamed(arg) | FunctionArg::Named { arg, .. } => Ok(arg.clone()),
            other => Err(ExecError::Message(format!(
                "unsupported function argument: {other:?}"
            ))),
        },
        other => Err(ExecError::Message(format!(
            "unsupported COUNT arguments: {other:?}"
        ))),
    }
}

fn compute_agg(func: &AggFn, rows: &[Row], table_columns: &[String]) -> Result<String, ExecError> {
    match func {
        AggFn::CountStar => Ok(rows.len().to_string()),
        AggFn::CountCol(col) => {
            let idx = column_index(table_columns, col)?;
            let n = rows.iter().filter(|r| !r[idx].is_empty()).count();
            Ok(n.to_string())
        }
        AggFn::Sum(col) => {
            let idx = column_index(table_columns, col)?;
            let mut sum: i64 = 0;
            let mut any = false;
            for r in rows {
                if !r[idx].is_empty() {
                    sum += r[idx]
                        .parse::<i64>()
                        .map_err(|_| ExecError::Message(format!("SUM on non-numeric '{col}'")))?;
                    any = true;
                }
            }
            Ok(if any { sum.to_string() } else { sql_null() })
        }
        AggFn::Min(col) => extremum(rows, table_columns, col, true),
        AggFn::Max(col) => extremum(rows, table_columns, col, false),
        AggFn::Avg(col) => {
            let idx = column_index(table_columns, col)?;
            let mut sum = 0.0f64;
            let mut n = 0u64;
            for r in rows {
                if !r[idx].is_empty() {
                    sum += r[idx]
                        .parse::<f64>()
                        .map_err(|_| ExecError::Message(format!("AVG on non-numeric '{col}'")))?;
                    n += 1;
                }
            }
            Ok(if n == 0 {
                sql_null()
            } else {
                format_avg(sum / n as f64)
            })
        }
    }
}

fn extremum(
    rows: &[Row],
    table_columns: &[String],
    col: &str,
    min: bool,
) -> Result<String, ExecError> {
    let idx = column_index(table_columns, col)?;
    let mut best: Option<String> = None;
    for r in rows {
        if r[idx].is_empty() {
            continue;
        }
        best = Some(match &best {
            None => r[idx].clone(),
            Some(cur) => {
                if compare_extremum(&r[idx], cur, min) {
                    r[idx].clone()
                } else {
                    cur.clone()
                }
            }
        });
    }
    Ok(best.unwrap_or_else(sql_null))
}

fn compare_extremum(a: &str, b: &str, min: bool) -> bool {
    if let (Ok(x), Ok(y)) = (a.parse::<i64>(), b.parse::<i64>()) {
        return if min { x < y } else { x > y };
    }
    if min {
        a < b
    } else {
        a > b
    }
}

fn format_avg(v: f64) -> String {
    let rounded = (v * 10000.0).round() / 10000.0;
    if (rounded - rounded.trunc()).abs() < f64::EPSILON {
        format!("{:.1}", rounded)
    } else {
        rounded.to_string()
    }
}

fn group_key(row: &Row, indices: &[usize]) -> String {
    indices
        .iter()
        .map(|i| row[*i].as_str())
        .collect::<Vec<_>>()
        .join("\x1f")
}

fn column_index(columns: &[String], name: &str) -> Result<usize, ExecError> {
    columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case(name))
        .ok_or_else(|| ExecError::Message(format!("unknown column '{name}'")))
}

fn expr_column_name(expr: &Expr) -> Result<String, ExecError> {
    match expr {
        Expr::Identifier(id) => Ok(id.value.clone()),
        Expr::CompoundIdentifier(parts) => parts
            .last()
            .map(|id| id.value.clone())
            .ok_or_else(|| ExecError::Message("empty compound identifier".into())),
        other => Err(ExecError::Message(format!(
            "unsupported column expression: {other:?}"
        ))),
    }
}

fn object_name_last(name: &ObjectName) -> Result<String, ExecError> {
    name.0
        .last()
        .map(|id| id.value.clone())
        .ok_or_else(|| ExecError::Message("empty function name".into()))
}

fn agg_display_name(func: &AggFn) -> String {
    match func {
        AggFn::CountStar => "COUNT(*)".into(),
        AggFn::CountCol(c) => format!("COUNT({c})"),
        AggFn::Sum(c) => format!("SUM({c})"),
        AggFn::Min(c) => format!("MIN({c})"),
        AggFn::Max(c) => format!("MAX({c})"),
        AggFn::Avg(c) => format!("AVG({c})"),
    }
}

fn parse_having(expr: &Expr, output_columns: &[String]) -> Result<WhereFilter, ExecError> {
    parse_having_or(expr, output_columns)
}

fn parse_having_or(expr: &Expr, output_columns: &[String]) -> Result<WhereFilter, ExecError> {
    if let Expr::BinaryOp {
        left,
        op: sqlparser::ast::BinaryOperator::Or,
        right,
    } = expr
    {
        return Ok(WhereFilter::Or(vec![
            parse_having_or(left, output_columns)?,
            parse_having_or(right, output_columns)?,
        ]));
    }
    parse_having_and(expr, output_columns)
}

fn parse_having_and(expr: &Expr, output_columns: &[String]) -> Result<WhereFilter, ExecError> {
    if let Expr::BinaryOp {
        left,
        op: sqlparser::ast::BinaryOperator::And,
        right,
    } = expr
    {
        return Ok(WhereFilter::And(vec![
            parse_having_and(left, output_columns)?,
            parse_having_and(right, output_columns)?,
        ]));
    }
    Ok(WhereFilter::Pred(parse_having_pred(expr, output_columns)?))
}

fn parse_having_pred(expr: &Expr, output_columns: &[String]) -> Result<Predicate, ExecError> {
    let Expr::BinaryOp { left, op, right } = expr else {
        return Err(ExecError::Message(format!(
            "unsupported HAVING expression: {expr:?}"
        )));
    };
    let column = having_column_name(left, output_columns)?;
    let op = match op {
        sqlparser::ast::BinaryOperator::Eq => CompareOp::Eq,
        sqlparser::ast::BinaryOperator::NotEq => CompareOp::NotEq,
        sqlparser::ast::BinaryOperator::Lt => CompareOp::Lt,
        sqlparser::ast::BinaryOperator::LtEq => CompareOp::LtEq,
        sqlparser::ast::BinaryOperator::Gt => CompareOp::Gt,
        sqlparser::ast::BinaryOperator::GtEq => CompareOp::GtEq,
        other => {
            return Err(ExecError::Message(format!(
                "unsupported HAVING operator: {other:?}"
            )))
        }
    };
    let value = having_literal(right)?;
    Ok(literal_predicate(column, op, value))
}

fn having_column_name(expr: &Expr, output_columns: &[String]) -> Result<String, ExecError> {
    match expr {
        Expr::Identifier(id) => {
            let name = &id.value;
            if output_columns.iter().any(|c| c.eq_ignore_ascii_case(name)) {
                Ok(output_columns
                    .iter()
                    .find(|c| c.eq_ignore_ascii_case(name))
                    .unwrap()
                    .clone())
            } else {
                Err(ExecError::Message(format!(
                    "unknown HAVING column '{name}'"
                )))
            }
        }
        Expr::Function(func) => {
            let display = function_display(func);
            if output_columns
                .iter()
                .any(|c| c.eq_ignore_ascii_case(&display))
            {
                Ok(output_columns
                    .iter()
                    .find(|c| c.eq_ignore_ascii_case(&display))
                    .unwrap()
                    .clone())
            } else {
                Err(ExecError::Message(format!(
                    "unknown HAVING aggregate '{display}'"
                )))
            }
        }
        other => Err(ExecError::Message(format!(
            "unsupported HAVING column: {other:?}"
        ))),
    }
}

fn function_display(func: &Function) -> String {
    format!("{}{}", func.name, func.args)
}

fn having_literal(expr: &Expr) -> Result<String, ExecError> {
    match expr {
        Expr::Value(sqlparser::ast::Value::Null) => Ok(String::new()),
        Expr::Value(sqlparser::ast::Value::Number(n, _)) => Ok(n.clone()),
        Expr::Value(sqlparser::ast::Value::SingleQuotedString(s)) => Ok(s.clone()),
        other => Err(ExecError::Message(format!(
            "unsupported HAVING literal: {other:?}"
        ))),
    }
}

fn sql_null() -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_sql::parse;
    use sqlparser::ast::{SetExpr, Statement};

    fn select_from(sql: &str) -> Select {
        let stmt = parse(sql).unwrap().into_iter().next().unwrap();
        let Statement::Query(q) = stmt else {
            panic!("expected query");
        };
        let SetExpr::Select(select) = q.body.as_ref() else {
            panic!("expected select");
        };
        (*select).as_ref().clone()
    }

    #[test]
    fn group_by_count_star() {
        let select =
            select_from("SELECT dept, COUNT(*) AS cnt FROM t GROUP BY dept HAVING cnt > 1");
        let cols = vec!["id".into(), "dept".into(), "salary".into()];
        let rows = vec![
            vec!["1".into(), "eng".into(), "100".into()],
            vec!["2".into(), "eng".into(), "200".into()],
            vec!["3".into(), "sales".into(), "150".into()],
        ];
        let (out_cols, out_rows) = execute_group_by(&select, &cols, rows).unwrap();
        assert_eq!(out_cols, vec!["dept", "cnt"]);
        assert_eq!(out_rows.len(), 1);
        assert_eq!(out_rows[0][0], "eng");
        assert_eq!(out_rows[0][1], "2");
    }

    #[test]
    fn group_by_sum_avg() {
        let select = select_from("SELECT dept, SUM(salary) AS total FROM t GROUP BY dept");
        let cols = vec!["dept".into(), "salary".into()];
        let rows = vec![
            vec!["eng".into(), "100".into()],
            vec!["eng".into(), "200".into()],
            vec!["sales".into(), "150".into()],
        ];
        let (out_cols, out_rows) = execute_group_by(&select, &cols, rows).unwrap();
        assert_eq!(out_cols, vec!["dept", "total"]);
        assert_eq!(out_rows.len(), 2);
        let eng = out_rows.iter().find(|r| r[0] == "eng").unwrap();
        assert_eq!(eng[1], "300");
    }
}

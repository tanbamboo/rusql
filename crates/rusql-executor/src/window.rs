//! Window ranking functions (M70): ROW_NUMBER, RANK, DENSE_RANK.

use crate::expr::{compare_for_expr, eval_expr};
use crate::ExecError;
use rusql_storage::Row;
use sqlparser::ast::{
    Expr, Function, FunctionArguments, Select, SelectItem, WindowSpec, WindowType,
};

#[derive(Debug, Clone, Copy)]
enum RankKind {
    RowNumber,
    Rank,
    DenseRank,
}

pub fn precompute(
    select: &Select,
    columns: &[String],
    rows: &[Row],
) -> Result<Vec<Option<Vec<String>>>, ExecError> {
    let mut out = Vec::with_capacity(select.projection.len());
    for item in &select.projection {
        match window_item(item)? {
            Some((kind, spec)) => out.push(Some(compute_ranks(kind, spec, columns, rows)?)),
            None => out.push(None),
        }
    }
    Ok(out)
}

fn window_item(item: &SelectItem) -> Result<Option<(RankKind, &WindowSpec)>, ExecError> {
    let expr = match item {
        SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => expr,
        _ => return Ok(None),
    };
    let Expr::Function(func) = expr else {
        return Ok(None);
    };
    let Some(over) = &func.over else {
        return Ok(None);
    };
    let spec = match over {
        WindowType::WindowSpec(spec) => spec,
        WindowType::NamedWindow(_) => {
            return Err(ExecError::Message(
                rusql_i18n::messages::sql_named_window_unsupported(),
            ));
        }
    };
    if spec.window_frame.is_some() {
        return Err(ExecError::Message(
            rusql_i18n::messages::sql_window_frame_unsupported(),
        ));
    }
    let kind = rank_kind(func)?;
    ensure_no_args(func)?;
    Ok(Some((kind, spec)))
}

fn rank_kind(func: &Function) -> Result<RankKind, ExecError> {
    let name = func.name.0.last().map(|id| id.value.as_str()).unwrap_or("");
    match name.to_ascii_uppercase().as_str() {
        "ROW_NUMBER" => Ok(RankKind::RowNumber),
        "RANK" => Ok(RankKind::Rank),
        "DENSE_RANK" => Ok(RankKind::DenseRank),
        other => Err(ExecError::Message(
            rusql_i18n::messages::sql_unsupported_window_function(other),
        )),
    }
}

fn ensure_no_args(func: &Function) -> Result<(), ExecError> {
    match &func.args {
        FunctionArguments::None => Ok(()),
        FunctionArguments::List(list) if list.args.is_empty() => Ok(()),
        FunctionArguments::List(_) | FunctionArguments::Subquery(_) => Err(ExecError::Message(
            rusql_i18n::messages::sql_window_rank_no_args(),
        )),
    }
}

fn compute_ranks(
    kind: RankKind,
    spec: &WindowSpec,
    columns: &[String],
    rows: &[Row],
) -> Result<Vec<String>, ExecError> {
    let mut values = vec!["0".to_string(); rows.len()];
    if rows.is_empty() {
        return Ok(values);
    }

    let mut groups: Vec<(Vec<String>, Vec<usize>)> = Vec::new();
    for (idx, row) in rows.iter().enumerate() {
        let key = eval_keys(row, columns, &spec.partition_by)?;
        if let Some(group) = groups.iter_mut().find(|(k, _)| *k == key) {
            group.1.push(idx);
        } else {
            groups.push((key, vec![idx]));
        }
    }

    for (_, members) in groups {
        let mut keyed: Vec<(Vec<String>, usize)> = Vec::with_capacity(members.len());
        for idx in members {
            keyed.push((eval_order_keys(&rows[idx], columns, spec)?, idx));
        }
        keyed.sort_by(|(a, _), (b, _)| compare_order_keys(a, b, spec));
        let mut rank: u64 = 1;
        let mut dense: u64 = 1;
        for (pos, (keys, idx)) in keyed.iter().enumerate() {
            if pos > 0 && *keys != keyed[pos - 1].0 {
                rank = (pos as u64) + 1;
                dense += 1;
            }
            let n = match kind {
                RankKind::RowNumber => (pos as u64) + 1,
                RankKind::Rank => rank,
                RankKind::DenseRank => dense,
            };
            values[*idx] = n.to_string();
        }
    }
    Ok(values)
}

fn eval_keys(row: &Row, columns: &[String], exprs: &[Expr]) -> Result<Vec<String>, ExecError> {
    exprs
        .iter()
        .map(|e| eval_expr(row, columns, e, None))
        .collect()
}

fn eval_order_keys(
    row: &Row,
    columns: &[String],
    spec: &WindowSpec,
) -> Result<Vec<String>, ExecError> {
    spec.order_by
        .iter()
        .map(|ob| eval_expr(row, columns, &ob.expr, None))
        .collect()
}

fn compare_order_keys(left: &[String], right: &[String], spec: &WindowSpec) -> std::cmp::Ordering {
    for (i, ob) in spec.order_by.iter().enumerate() {
        let cmp = compare_for_expr(&left[i], &right[i]);
        let ord = match cmp {
            n if n < 0 => std::cmp::Ordering::Less,
            n if n > 0 => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        };
        let ord = if ob.asc.unwrap_or(true) {
            ord
        } else {
            ord.reverse()
        };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    std::cmp::Ordering::Equal
}

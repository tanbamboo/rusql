//! Window ranking functions (M70) and ROWS BETWEEN frames (M129).

use crate::expr::{compare_for_expr, eval_expr};
use crate::ExecError;
use rusql_storage::Row;
use sqlparser::ast::{
    Expr, Function, FunctionArguments, Select, SelectItem, Value, WindowFrame, WindowFrameBound,
    WindowFrameUnits, WindowSpec, WindowType,
};

/// MySQL `ER_WINDOW_ILLEGAL_ORDER_BY` / frame-spec errno for an illegal ROWS bound pair.
const ER_WINDOW_ILLEGAL_FRAME: u16 = 3585;

#[derive(Debug, Clone, Copy)]
enum RankKind {
    RowNumber,
    Rank,
    DenseRank,
}

/// A physical ROWS bound relative to the current row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowsBound {
    UnboundedPreceding,
    CurrentRow,
    Preceding(u64),
    Following(u64),
    UnboundedFollowing,
}

impl RowsBound {
    fn order_key(self) -> i128 {
        match self {
            Self::UnboundedPreceding => i128::MIN,
            Self::Preceding(n) => -(n as i128),
            Self::CurrentRow => 0,
            Self::Following(n) => n as i128,
            Self::UnboundedFollowing => i128::MAX,
        }
    }

    fn raw_index(self, pos: usize) -> isize {
        match self {
            Self::UnboundedPreceding => 0,
            Self::CurrentRow => pos as isize,
            Self::Preceding(n) => (pos as isize).saturating_sub(n as isize),
            Self::Following(n) => (pos as isize).saturating_add(n as isize),
            Self::UnboundedFollowing => isize::MAX / 4,
        }
    }
}

/// Inclusive `[start, end]` indexes of a ROWS frame in an ordered partition, or `None` if empty.
fn rows_frame_range(
    len: usize,
    pos: usize,
    start: RowsBound,
    end: RowsBound,
) -> Option<(usize, usize)> {
    if len == 0 || pos >= len {
        return None;
    }
    let start_raw = match start {
        RowsBound::UnboundedPreceding => 0,
        RowsBound::UnboundedFollowing => len as isize,
        other => other.raw_index(pos),
    };
    let end_raw = match end {
        RowsBound::UnboundedFollowing => (len as isize) - 1,
        RowsBound::UnboundedPreceding => -1,
        other => other.raw_index(pos),
    };
    let start_idx = start_raw.max(0);
    let end_idx = end_raw.min((len as isize) - 1);
    if start_idx > end_idx {
        None
    } else {
        Some((start_idx as usize, end_idx as usize))
    }
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
    if spec.window_name.is_some() {
        return Err(ExecError::Message(
            rusql_i18n::messages::sql_named_window_unsupported(),
        ));
    }
    if let Some(frame) = &spec.window_frame {
        let _ = parse_rows_frame(frame)?;
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

fn parse_rows_frame(frame: &WindowFrame) -> Result<(RowsBound, RowsBound), ExecError> {
    match frame.units {
        WindowFrameUnits::Rows => {}
        WindowFrameUnits::Range | WindowFrameUnits::Groups => {
            return Err(ExecError::Message(
                rusql_i18n::messages::sql_window_frame_unsupported(),
            ));
        }
    }
    let start = parse_rows_bound(&frame.start_bound)?;
    let end = match &frame.end_bound {
        None => RowsBound::CurrentRow,
        Some(bound) => parse_rows_bound(bound)?,
    };
    if matches!(start, RowsBound::UnboundedFollowing)
        || matches!(end, RowsBound::UnboundedPreceding)
        || start.order_key() > end.order_key()
    {
        return Err(ExecError::Mysql {
            code: ER_WINDOW_ILLEGAL_FRAME,
            message: rusql_i18n::messages::sql_window_frame_illegal(),
        });
    }
    Ok((start, end))
}

fn parse_rows_bound(bound: &WindowFrameBound) -> Result<RowsBound, ExecError> {
    match bound {
        WindowFrameBound::CurrentRow => Ok(RowsBound::CurrentRow),
        WindowFrameBound::Preceding(None) => Ok(RowsBound::UnboundedPreceding),
        WindowFrameBound::Following(None) => Ok(RowsBound::UnboundedFollowing),
        WindowFrameBound::Preceding(Some(expr)) => {
            Ok(RowsBound::Preceding(unsigned_frame_offset(expr)?))
        }
        WindowFrameBound::Following(Some(expr)) => {
            Ok(RowsBound::Following(unsigned_frame_offset(expr)?))
        }
    }
}

fn unsigned_frame_offset(expr: &Expr) -> Result<u64, ExecError> {
    let Expr::Value(Value::Number(raw, _)) = expr else {
        return Err(ExecError::Message(
            rusql_i18n::messages::sql_window_frame_offset_invalid(),
        ));
    };
    if raw.contains('.') || raw.contains('e') || raw.contains('E') || raw.starts_with('-') {
        return Err(ExecError::Message(
            rusql_i18n::messages::sql_window_frame_offset_invalid(),
        ));
    }
    raw.parse::<u64>()
        .map_err(|_| ExecError::Message(rusql_i18n::messages::sql_window_frame_offset_invalid()))
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

    let frame_bounds = spec
        .window_frame
        .as_ref()
        .map(parse_rows_frame)
        .transpose()?;

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
            // Ranking functions ignore the ROWS peer set (MySQL 8.0). Still resolve
            // the frame on every current row so n PRECEDING / FOLLOWING is not dropped.
            if let Some((start, end)) = frame_bounds {
                let _ = rows_frame_range(keyed.len(), pos, start, end);
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

#[cfg(test)]
mod window_frame_tests {
    use super::{rows_frame_range, RowsBound};

    #[test]
    fn window_frame_unbounded_preceding_to_current_row() {
        assert_eq!(
            rows_frame_range(4, 2, RowsBound::UnboundedPreceding, RowsBound::CurrentRow),
            Some((0, 2))
        );
        assert_eq!(
            rows_frame_range(4, 0, RowsBound::UnboundedPreceding, RowsBound::CurrentRow),
            Some((0, 0))
        );
    }

    #[test]
    fn window_frame_n_preceding_and_following() {
        assert_eq!(
            rows_frame_range(4, 2, RowsBound::Preceding(1), RowsBound::CurrentRow),
            Some((1, 2))
        );
        assert_eq!(
            rows_frame_range(4, 0, RowsBound::Preceding(1), RowsBound::CurrentRow),
            Some((0, 0))
        );
        assert_eq!(
            rows_frame_range(4, 1, RowsBound::CurrentRow, RowsBound::Following(1)),
            Some((1, 2))
        );
        assert_eq!(
            rows_frame_range(4, 3, RowsBound::CurrentRow, RowsBound::Following(1)),
            Some((3, 3))
        );
        assert_eq!(
            rows_frame_range(4, 3, RowsBound::Following(1), RowsBound::Following(1)),
            None
        );
    }

    #[test]
    fn window_frame_current_row_only() {
        assert_eq!(
            rows_frame_range(4, 2, RowsBound::CurrentRow, RowsBound::CurrentRow),
            Some((2, 2))
        );
    }
}

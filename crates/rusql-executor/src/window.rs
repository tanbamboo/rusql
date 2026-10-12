//! Window ranking (M70), ROWS BETWEEN frames (M129), and LAG / LEAD / SUM() OVER (M135).

use crate::expr::{compare_for_expr, eval_expr};
use crate::ExecError;
use rusql_storage::Row;
use sqlparser::ast::{
    Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, Select, SelectItem, Value,
    WindowFrame, WindowFrameBound, WindowFrameUnits, WindowSpec, WindowType,
};

/// MySQL `ER_WRONG_PARAMCOUNT_TO_NATIVE_FCT`.
const ER_WRONG_PARAMCOUNT: u16 = 1582;
/// MySQL `ER_WINDOW_ILLEGAL_ORDER_BY` / frame-spec errno for an illegal ROWS bound pair.
const ER_WINDOW_ILLEGAL_FRAME: u16 = 3585;

#[derive(Debug, Clone, Copy)]
enum RankKind {
    RowNumber,
    Rank,
    DenseRank,
}

#[derive(Debug, Clone, Copy)]
enum OffsetDir {
    Lag,
    Lead,
}

#[derive(Debug, Clone)]
enum WindowKind {
    Rank(RankKind),
    Offset {
        dir: OffsetDir,
        expr: Box<Expr>,
        offset: u64,
        default: Option<Box<Expr>>,
    },
    Sum {
        expr: Box<Expr>,
    },
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
            Some((kind, spec)) => out.push(Some(compute_window(&kind, spec, columns, rows)?)),
            None => out.push(None),
        }
    }
    Ok(out)
}

fn window_item(item: &SelectItem) -> Result<Option<(WindowKind, &WindowSpec)>, ExecError> {
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
    let kind = window_kind(func)?;
    Ok(Some((kind, spec)))
}

fn window_kind(func: &Function) -> Result<WindowKind, ExecError> {
    let name = func.name.0.last().map(|id| id.value.as_str()).unwrap_or("");
    match name.to_ascii_uppercase().as_str() {
        "ROW_NUMBER" => {
            ensure_no_args(func)?;
            Ok(WindowKind::Rank(RankKind::RowNumber))
        }
        "RANK" => {
            ensure_no_args(func)?;
            Ok(WindowKind::Rank(RankKind::Rank))
        }
        "DENSE_RANK" => {
            ensure_no_args(func)?;
            Ok(WindowKind::Rank(RankKind::DenseRank))
        }
        "LAG" => parse_offset_fn(func, OffsetDir::Lag),
        "LEAD" => parse_offset_fn(func, OffsetDir::Lead),
        "SUM" => parse_sum_fn(func),
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

fn parse_offset_fn(func: &Function, dir: OffsetDir) -> Result<WindowKind, ExecError> {
    let name = func.name.0.last().map(|id| id.value.as_str()).unwrap_or("");
    let args = function_expr_args(func, name)?;
    if args.is_empty() || args.len() > 3 {
        return Err(wrong_param_count(name));
    }
    let expr = args[0].clone();
    let offset = if args.len() >= 2 {
        unsigned_offset(args[1])?
    } else {
        1
    };
    let default = if args.len() == 3 {
        Some(args[2].clone())
    } else {
        None
    };
    Ok(WindowKind::Offset {
        dir,
        expr: Box::new(expr),
        offset,
        default: default.map(Box::new),
    })
}

fn parse_sum_fn(func: &Function) -> Result<WindowKind, ExecError> {
    let args = function_expr_args(func, "SUM")?;
    if args.len() != 1 {
        return Err(wrong_param_count("SUM"));
    }
    Ok(WindowKind::Sum {
        expr: Box::new(args[0].clone()),
    })
}

fn function_expr_args<'a>(func: &'a Function, name: &str) -> Result<Vec<&'a Expr>, ExecError> {
    match &func.args {
        FunctionArguments::None => Ok(Vec::new()),
        FunctionArguments::List(list) => {
            let mut out = Vec::with_capacity(list.args.len());
            for arg in &list.args {
                match arg {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(expr))
                    | FunctionArg::Named {
                        arg: FunctionArgExpr::Expr(expr),
                        ..
                    } => out.push(expr),
                    _ => return Err(wrong_param_count(name)),
                }
            }
            Ok(out)
        }
        FunctionArguments::Subquery(_) => Err(wrong_param_count(name)),
    }
}

fn wrong_param_count(name: &str) -> ExecError {
    ExecError::Mysql {
        code: ER_WRONG_PARAMCOUNT,
        message: rusql_i18n::messages::sql_incorrect_parameter_count(name),
    }
}

fn unsigned_offset(expr: &Expr) -> Result<u64, ExecError> {
    let Expr::Value(Value::Number(raw, _)) = expr else {
        return Err(ExecError::Message(
            rusql_i18n::messages::sql_window_offset_invalid(),
        ));
    };
    if raw.contains('.') || raw.contains('e') || raw.contains('E') || raw.starts_with('-') {
        return Err(ExecError::Message(
            rusql_i18n::messages::sql_window_offset_invalid(),
        ));
    }
    raw.parse::<u64>()
        .map_err(|_| ExecError::Message(rusql_i18n::messages::sql_window_offset_invalid()))
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

fn compute_window(
    kind: &WindowKind,
    spec: &WindowSpec,
    columns: &[String],
    rows: &[Row],
) -> Result<Vec<String>, ExecError> {
    let mut values = vec![String::new(); rows.len()];
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
        let mut work = PartitionWork {
            keyed: &keyed,
            frame_bounds,
            columns,
            rows,
            values: &mut values,
        };
        match kind {
            WindowKind::Rank(rank_kind) => fill_ranks(*rank_kind, &mut work),
            WindowKind::Offset {
                dir,
                expr,
                offset,
                default,
            } => fill_offset(*dir, expr, *offset, default.as_deref(), &mut work)?,
            WindowKind::Sum { expr } => fill_sum(expr, spec, &mut work)?,
        }
    }
    Ok(values)
}

struct PartitionWork<'a> {
    keyed: &'a [(Vec<String>, usize)],
    frame_bounds: Option<(RowsBound, RowsBound)>,
    columns: &'a [String],
    rows: &'a [Row],
    values: &'a mut [String],
}

fn fill_ranks(kind: RankKind, work: &mut PartitionWork<'_>) {
    let mut rank: u64 = 1;
    let mut dense: u64 = 1;
    for (pos, (keys, idx)) in work.keyed.iter().enumerate() {
        if pos > 0 && *keys != work.keyed[pos - 1].0 {
            rank = (pos as u64) + 1;
            dense += 1;
        }
        // Ranking functions ignore the ROWS peer set (MySQL 8.0). Still resolve
        // the frame on every current row so n PRECEDING / FOLLOWING is not dropped.
        if let Some((start, end)) = work.frame_bounds {
            let _ = rows_frame_range(work.keyed.len(), pos, start, end);
        }
        let n = match kind {
            RankKind::RowNumber => (pos as u64) + 1,
            RankKind::Rank => rank,
            RankKind::DenseRank => dense,
        };
        work.values[*idx] = n.to_string();
    }
}

fn fill_offset(
    dir: OffsetDir,
    expr: &Expr,
    offset: u64,
    default: Option<&Expr>,
    work: &mut PartitionWork<'_>,
) -> Result<(), ExecError> {
    for (pos, (_, idx)) in work.keyed.iter().enumerate() {
        if let Some((start, end)) = work.frame_bounds {
            let _ = rows_frame_range(work.keyed.len(), pos, start, end);
        }
        let target = match dir {
            OffsetDir::Lag => (pos as i128).checked_sub(offset as i128),
            OffsetDir::Lead => (pos as i128).checked_add(offset as i128),
        };
        let in_range = target
            .filter(|t| *t >= 0 && (*t as usize) < work.keyed.len())
            .map(|t| t as usize);
        work.values[*idx] = if let Some(tpos) = in_range {
            eval_expr(&work.rows[work.keyed[tpos].1], work.columns, expr, None)?
        } else if let Some(def) = default {
            eval_expr(&work.rows[*idx], work.columns, def, None)?
        } else {
            String::new()
        };
    }
    Ok(())
}

fn fill_sum(expr: &Expr, spec: &WindowSpec, work: &mut PartitionWork<'_>) -> Result<(), ExecError> {
    let (start, end) = match work.frame_bounds {
        Some(bounds) => bounds,
        None if spec.order_by.is_empty() => {
            (RowsBound::UnboundedPreceding, RowsBound::UnboundedFollowing)
        }
        None => (RowsBound::UnboundedPreceding, RowsBound::CurrentRow),
    };
    let mut cells = Vec::with_capacity(work.keyed.len());
    for (_, idx) in work.keyed {
        cells.push(eval_expr(&work.rows[*idx], work.columns, expr, None)?);
    }
    for pos in 0..work.keyed.len() {
        let out_idx = work.keyed[pos].1;
        work.values[out_idx] = match rows_frame_range(work.keyed.len(), pos, start, end) {
            Some((lo, hi)) => sum_cells(&cells[lo..=hi])?,
            None => String::new(),
        };
    }
    Ok(())
}

fn sum_cells(cells: &[String]) -> Result<String, ExecError> {
    let mut sum: i128 = 0;
    let mut any = false;
    for cell in cells {
        if cell.is_empty() {
            continue;
        }
        let n = cell
            .parse::<i128>()
            .map_err(|_| ExecError::Message(rusql_i18n::messages::sql_window_sum_non_numeric()))?;
        sum += n;
        any = true;
    }
    Ok(if any { sum.to_string() } else { String::new() })
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

#[cfg(test)]
mod lag_lead_tests {
    use super::precompute;
    use crate::ExecError;
    use rusql_sql::parse;
    use rusql_storage::Row;
    use sqlparser::ast::{SetExpr, Statement};

    fn fixture_rows() -> (Vec<String>, Vec<Row>) {
        (
            vec!["id".into(), "g".into(), "n".into()],
            vec![
                vec!["1".into(), "a".into(), "10".into()],
                vec!["2".into(), "a".into(), "20".into()],
                vec!["3".into(), "b".into(), "5".into()],
            ],
        )
    }

    fn window_col(sql: &str) -> Result<Vec<String>, ExecError> {
        let stmt = parse(sql).expect("parse").into_iter().next().expect("stmt");
        let Statement::Query(q) = stmt else {
            panic!("expected query");
        };
        let SetExpr::Select(select) = q.body.as_ref() else {
            panic!("expected select");
        };
        let (cols, rows) = fixture_rows();
        let out = precompute(select, &cols, &rows)?;
        Ok(out
            .into_iter()
            .find_map(|c| c)
            .expect("expected a window projection"))
    }

    #[test]
    fn lag_lead_default_offset_null_edges() {
        assert_eq!(
            window_col("SELECT LAG(n) OVER (ORDER BY id) FROM t").unwrap(),
            vec!["".to_string(), "10".to_string(), "20".to_string()]
        );
        assert_eq!(
            window_col("SELECT LEAD(n) OVER (ORDER BY id) FROM t").unwrap(),
            vec!["20".to_string(), "5".to_string(), "".to_string()]
        );
    }

    #[test]
    fn lag_lead_explicit_offset_one_matches_default() {
        assert_eq!(
            window_col("SELECT LAG(n, 1) OVER (ORDER BY id) FROM t").unwrap(),
            window_col("SELECT LAG(n) OVER (ORDER BY id) FROM t").unwrap()
        );
        assert_eq!(
            window_col("SELECT LEAD(n, 1) OVER (ORDER BY id) FROM t").unwrap(),
            window_col("SELECT LEAD(n) OVER (ORDER BY id) FROM t").unwrap()
        );
    }

    #[test]
    fn lag_lead_custom_default_and_offset_two() {
        assert_eq!(
            window_col("SELECT LAG(n, 1, 0) OVER (ORDER BY id) FROM t").unwrap(),
            vec!["0".to_string(), "10".to_string(), "20".to_string()]
        );
        assert_eq!(
            window_col("SELECT LAG(n, 2) OVER (ORDER BY id) FROM t").unwrap(),
            vec!["".to_string(), "".to_string(), "10".to_string()]
        );
    }

    #[test]
    fn lag_lead_wrong_arity_errno_1582() {
        match window_col("SELECT LAG() OVER (ORDER BY id) FROM t") {
            Err(ExecError::Mysql { code, .. }) => assert_eq!(code, 1582),
            other => panic!("expected errno 1582, got {other:?}"),
        }
        match window_col("SELECT LEAD(n, 1, 0, 9) OVER (ORDER BY id) FROM t") {
            Err(ExecError::Mysql { code, .. }) => assert_eq!(code, 1582),
            other => panic!("expected errno 1582, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod sum_over_tests {
    use super::precompute;
    use crate::ExecError;
    use rusql_sql::parse;
    use rusql_storage::Row;
    use sqlparser::ast::{SetExpr, Statement};

    fn fixture_rows() -> (Vec<String>, Vec<Row>) {
        (
            vec!["id".into(), "g".into(), "n".into()],
            vec![
                vec!["1".into(), "a".into(), "10".into()],
                vec!["2".into(), "a".into(), "20".into()],
                vec!["3".into(), "b".into(), "5".into()],
            ],
        )
    }

    fn window_col(sql: &str) -> Result<Vec<String>, ExecError> {
        let stmt = parse(sql).expect("parse").into_iter().next().expect("stmt");
        let Statement::Query(q) = stmt else {
            panic!("expected query");
        };
        let SetExpr::Select(select) = q.body.as_ref() else {
            panic!("expected select");
        };
        let (cols, rows) = fixture_rows();
        let out = precompute(select, &cols, &rows)?;
        Ok(out
            .into_iter()
            .find_map(|c| c)
            .expect("expected a window projection"))
    }

    #[test]
    fn sum_over_running_sum_per_partition() {
        assert_eq!(
            window_col("SELECT SUM(n) OVER (PARTITION BY g ORDER BY id) FROM t").unwrap(),
            vec!["10".to_string(), "30".to_string(), "5".to_string()]
        );
    }

    #[test]
    fn sum_over_without_order_by_is_partition_total() {
        assert_eq!(
            window_col("SELECT SUM(n) OVER (PARTITION BY g) FROM t").unwrap(),
            vec!["30".to_string(), "30".to_string(), "5".to_string()]
        );
    }

    #[test]
    fn sum_over_rows_frame_honors_bounds() {
        assert_eq!(
            window_col(
                "SELECT SUM(n) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t"
            )
            .unwrap(),
            vec!["10".to_string(), "30".to_string(), "25".to_string()]
        );
    }

    #[test]
    fn sum_over_ranking_unchanged() {
        assert_eq!(
            window_col("SELECT ROW_NUMBER() OVER (ORDER BY id) FROM t").unwrap(),
            vec!["1".to_string(), "2".to_string(), "3".to_string()]
        );
    }
}

//! SQL expression evaluation (M46).

use crate::ExecError;
use rusql_core::Session;
use rusql_storage::Row;
use sqlparser::ast::{
    BinaryOperator, CastKind, DataType, Expr, Function, FunctionArg, FunctionArgExpr,
    FunctionArguments, Value,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn eval_expr(
    row: &Row,
    columns: &[String],
    expr: &Expr,
    session: Option<&Session>,
) -> Result<String, ExecError> {
    match expr {
        Expr::Value(v) => value_to_string(v),
        Expr::Identifier(id) => cell_value(row, columns, &id.value),
        Expr::CompoundIdentifier(parts) => {
            let name = parts
                .last()
                .map(|id| id.value.as_str())
                .ok_or_else(|| ExecError::Message("empty compound identifier".into()))?;
            cell_value(row, columns, name)
        }
        Expr::BinaryOp { left, op, right } => eval_binary(row, columns, left, op, right, session),
        Expr::UnaryOp {
            op: sqlparser::ast::UnaryOperator::Minus,
            expr: inner,
        } => {
            let v = eval_expr(row, columns, inner, session)?;
            let n: i64 = v
                .parse()
                .map_err(|_| ExecError::Message("unary minus on non-numeric".into()))?;
            Ok((-n).to_string())
        }
        Expr::Function(func) => eval_function(row, columns, func, session),
        Expr::Cast {
            expr: inner,
            data_type,
            kind,
            ..
        } => eval_cast(row, columns, inner, data_type, kind, session),
        Expr::Nested(inner) => eval_expr(row, columns, inner, session),
        other => Err(ExecError::Message(format!(
            "unsupported expression: {other:?}"
        ))),
    }
}

pub(crate) fn expr_output_name(expr: &Expr, alias: Option<&str>) -> Result<String, ExecError> {
    if let Some(a) = alias {
        return Ok(a.to_string());
    }
    match expr {
        Expr::Identifier(id) => Ok(id.value.clone()),
        Expr::CompoundIdentifier(parts) => parts
            .last()
            .map(|id| id.value.clone())
            .ok_or_else(|| ExecError::Message("empty compound identifier".into())),
        Expr::Function(func) => Ok(format!("{}{}", func.name, func.args)),
        Expr::BinaryOp { .. } => Ok("expr".into()),
        Expr::Cast { expr: inner, .. } => expr_output_name(inner, None),
        other => Err(ExecError::Message(format!(
            "unsupported SELECT expression: {other:?}"
        ))),
    }
}

fn eval_binary(
    row: &Row,
    columns: &[String],
    left: &Expr,
    op: &BinaryOperator,
    right: &Expr,
    session: Option<&Session>,
) -> Result<String, ExecError> {
    if *op == BinaryOperator::StringConcat {
        let l = eval_expr(row, columns, left, session)?;
        let r = eval_expr(row, columns, right, session)?;
        return Ok(format!("{l}{r}"));
    }
    let l = eval_expr(row, columns, left, session)?;
    let r = eval_expr(row, columns, right, session)?;
    if is_nullish(&l) || is_nullish(&r) {
        return Ok(String::new());
    }
    match op {
        BinaryOperator::Plus => num_op(&l, &r, |a, b| a + b),
        BinaryOperator::Minus => num_op(&l, &r, |a, b| a - b),
        BinaryOperator::Multiply => num_op(&l, &r, |a, b| a * b),
        BinaryOperator::Divide => {
            let a: f64 = l
                .parse()
                .map_err(|_| ExecError::Message("divide non-numeric".into()))?;
            let b: f64 = r
                .parse()
                .map_err(|_| ExecError::Message("divide non-numeric".into()))?;
            if b == 0.0 {
                return Err(ExecError::Message("division by zero".into()));
            }
            Ok((a / b).to_string())
        }
        other => Err(ExecError::Message(format!(
            "unsupported binary operator: {other:?}"
        ))),
    }
}

fn num_op<F>(l: &str, r: &str, f: F) -> Result<String, ExecError>
where
    F: Fn(i64, i64) -> i64,
{
    let a: i64 = l
        .parse()
        .map_err(|_| ExecError::Message("arithmetic on non-numeric".into()))?;
    let b: i64 = r
        .parse()
        .map_err(|_| ExecError::Message("arithmetic on non-numeric".into()))?;
    Ok(f(a, b).to_string())
}

fn eval_function(
    row: &Row,
    columns: &[String],
    func: &Function,
    session: Option<&Session>,
) -> Result<String, ExecError> {
    let raw_name = func
        .name
        .0
        .last()
        .map(|id| id.value.as_str())
        .ok_or_else(|| ExecError::Message("empty function name".into()))?;
    if let Some(sess) = session {
        if let Some(udf) = sess.catalog.get_function(&sess.database, raw_name) {
            return eval_user_function(row, columns, udf, session);
        }
    }
    let name = raw_name.to_ascii_uppercase();
    match name.as_str() {
        "CONCAT" => eval_concat(row, columns, func, session),
        "COALESCE" | "IFNULL" => eval_coalesce(row, columns, func, session),
        "NULLIF" => eval_nullif(row, columns, func, session),
        "NOW" => Ok(now_string()),
        "CURDATE" => Ok(curdate_string()),
        "LENGTH" => {
            let arg = single_arg(row, columns, func, session)?;
            Ok(arg.len().to_string())
        }
        "LOWER" => {
            let arg = single_arg(row, columns, func, session)?;
            Ok(arg.to_ascii_lowercase())
        }
        "UPPER" => {
            let arg = single_arg(row, columns, func, session)?;
            Ok(arg.to_ascii_uppercase())
        }
        other => Err(ExecError::Message(format!("unsupported function: {other}"))),
    }
}

fn eval_user_function(
    row: &Row,
    columns: &[String],
    func: &rusql_core::FunctionMeta,
    session: Option<&Session>,
) -> Result<String, ExecError> {
    let sql = format!("SELECT {}", func.return_expr);
    let stmt = rusql_sql::parse(&sql)
        .map_err(|e| ExecError::Message(e.to_string()))?
        .into_iter()
        .next()
        .ok_or_else(|| ExecError::Message("empty function body".into()))?;
    let sqlparser::ast::Statement::Query(q) = stmt else {
        return Err(ExecError::Message("invalid function body".into()));
    };
    let sqlparser::ast::SetExpr::Select(select) = q.body.as_ref() else {
        return Err(ExecError::Message("invalid function body".into()));
    };
    let sqlparser::ast::SelectItem::UnnamedExpr(expr) = &select.projection[0] else {
        return Err(ExecError::Message("invalid function body".into()));
    };
    eval_expr(row, columns, expr, session)
}

fn eval_concat(
    row: &Row,
    columns: &[String],
    func: &Function,
    session: Option<&Session>,
) -> Result<String, ExecError> {
    let args = function_args(row, columns, func, session)?;
    Ok(args.join(""))
}

fn eval_coalesce(
    row: &Row,
    columns: &[String],
    func: &Function,
    session: Option<&Session>,
) -> Result<String, ExecError> {
    for arg in function_args(row, columns, func, session)? {
        if !is_nullish(&arg) {
            return Ok(arg);
        }
    }
    Ok(String::new())
}

fn eval_nullif(
    row: &Row,
    columns: &[String],
    func: &Function,
    session: Option<&Session>,
) -> Result<String, ExecError> {
    let args = function_args(row, columns, func, session)?;
    if args.len() != 2 {
        return Err(ExecError::Message("NULLIF requires two arguments".into()));
    }
    if args[0] == args[1] {
        Ok(String::new())
    } else {
        Ok(args[0].clone())
    }
}

fn eval_cast(
    row: &Row,
    columns: &[String],
    inner: &Expr,
    data_type: &DataType,
    kind: &CastKind,
    session: Option<&Session>,
) -> Result<String, ExecError> {
    let _ = kind;
    let v = eval_expr(row, columns, inner, session)?;
    if is_nullish(&v) {
        return Ok(String::new());
    }
    match data_type {
        DataType::Int(_) | DataType::Integer(_) | DataType::BigInt(_) | DataType::SmallInt(_) => {
            let n: i64 = v
                .parse()
                .map_err(|_| ExecError::Message("CAST to INT failed".into()))?;
            Ok(n.to_string())
        }
        DataType::Varchar(_) | DataType::Text | DataType::Char(_) | DataType::String(_) => Ok(v),
        DataType::Decimal(_) | DataType::Numeric(_) => {
            let n: f64 = v
                .parse()
                .map_err(|_| ExecError::Message("CAST to DECIMAL failed".into()))?;
            Ok(n.to_string())
        }
        other => Err(ExecError::Message(format!(
            "unsupported CAST target type: {other:?}"
        ))),
    }
}

fn function_args(
    row: &Row,
    columns: &[String],
    func: &Function,
    session: Option<&Session>,
) -> Result<Vec<String>, ExecError> {
    match &func.args {
        FunctionArguments::List(list) => list
            .args
            .iter()
            .map(|arg| match arg {
                FunctionArg::Unnamed(arg) | FunctionArg::Named { arg, .. } => match arg {
                    FunctionArgExpr::Expr(expr) => eval_expr(row, columns, expr, session),
                    other => Err(ExecError::Message(format!(
                        "unsupported function argument: {other:?}"
                    ))),
                },
                other => Err(ExecError::Message(format!(
                    "unsupported function argument: {other:?}"
                ))),
            })
            .collect(),
        FunctionArguments::None => Ok(vec![]),
        other => Err(ExecError::Message(format!(
            "unsupported function arguments: {other:?}"
        ))),
    }
}

fn single_arg(
    row: &Row,
    columns: &[String],
    func: &Function,
    session: Option<&Session>,
) -> Result<String, ExecError> {
    let args = function_args(row, columns, func, session)?;
    args.into_iter()
        .next()
        .ok_or_else(|| ExecError::Message("function missing argument".into()))
}

fn cell_value(row: &Row, columns: &[String], name: &str) -> Result<String, ExecError> {
    let idx = columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case(name))
        .ok_or_else(|| ExecError::Message(format!("unknown column '{name}'")))?;
    Ok(row.get(idx).cloned().unwrap_or_default())
}

fn value_to_string(v: &Value) -> Result<String, ExecError> {
    match v {
        Value::Null => Ok(String::new()),
        Value::Number(n, _) => Ok(n.clone()),
        Value::SingleQuotedString(s) => Ok(s.clone()),
        other => Err(ExecError::Message(format!("unsupported value: {other:?}"))),
    }
}

fn is_nullish(v: &str) -> bool {
    v.is_empty()
}

fn now_string() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_timestamp(secs)
}

fn curdate_string() -> String {
    now_string()
        .split_whitespace()
        .next()
        .unwrap_or("1970-01-01")
        .to_string()
}

fn format_timestamp(secs: u64) -> String {
    let days = secs / 86400;
    let time = secs % 86400;
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02} {h:02}:{min:02}:{s:02}",
        h = time / 3600,
        min = (time % 3600) / 60,
        s = time % 60
    )
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_sql::parse;
    use sqlparser::ast::{SelectItem, SetExpr, Statement};

    fn eval_sql(sql: &str, row: Row, cols: &[&str]) -> String {
        let stmt = parse(sql).unwrap().into_iter().next().unwrap();
        let Statement::Query(q) = stmt else {
            panic!("expected query");
        };
        let SetExpr::Select(select) = q.body.as_ref() else {
            panic!("expected select");
        };
        let SelectItem::UnnamedExpr(expr) = &select.projection[0] else {
            panic!("expected expr");
        };
        eval_expr(
            &row,
            &cols.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            expr,
            None,
        )
        .unwrap()
    }

    #[test]
    fn arithmetic_add() {
        assert_eq!(
            eval_sql("SELECT id + 1 FROM t", vec!["5".into()], &["id"]),
            "6"
        );
    }

    #[test]
    fn concat_fn() {
        assert_eq!(
            eval_sql(
                "SELECT CONCAT(name, '!') FROM t",
                vec!["hi".into()],
                &["name"]
            ),
            "hi!"
        );
    }

    #[test]
    fn coalesce() {
        assert_eq!(
            eval_sql(
                "SELECT COALESCE(note, 'x') FROM t",
                vec!["".into()],
                &["note"]
            ),
            "x"
        );
    }
}

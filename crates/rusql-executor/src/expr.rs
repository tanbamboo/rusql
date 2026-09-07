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
        Expr::Case {
            operand,
            conditions,
            results,
            else_result,
        } => eval_case(
            row,
            columns,
            operand.as_deref(),
            conditions,
            results,
            else_result.as_deref(),
            session,
        ),
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
        Expr::Case { .. } => Ok("CASE".into()),
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
    if matches!(
        op,
        BinaryOperator::And | BinaryOperator::Or | BinaryOperator::Xor
    ) {
        let l = eval_expr(row, columns, left, session)?;
        let r = eval_expr(row, columns, right, session)?;
        let result = match op {
            BinaryOperator::And => is_truthy(&l) && is_truthy(&r),
            BinaryOperator::Or => is_truthy(&l) || is_truthy(&r),
            BinaryOperator::Xor => is_truthy(&l) != is_truthy(&r),
            _ => unreachable!(),
        };
        return Ok(if result { "1".into() } else { "0".into() });
    }
    let l = eval_expr(row, columns, left, session)?;
    let r = eval_expr(row, columns, right, session)?;
    if matches!(
        op,
        BinaryOperator::Eq
            | BinaryOperator::NotEq
            | BinaryOperator::Lt
            | BinaryOperator::LtEq
            | BinaryOperator::Gt
            | BinaryOperator::GtEq
    ) {
        if is_nullish(&l) || is_nullish(&r) {
            return Ok(String::new());
        }
        let cmp = compare_for_expr(&l, &r);
        let result = match op {
            BinaryOperator::Eq => cmp == 0,
            BinaryOperator::NotEq => cmp != 0,
            BinaryOperator::Lt => cmp < 0,
            BinaryOperator::LtEq => cmp <= 0,
            BinaryOperator::Gt => cmp > 0,
            BinaryOperator::GtEq => cmp >= 0,
            _ => unreachable!(),
        };
        return Ok(if result { "1".into() } else { "0".into() });
    }
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

fn compare_for_expr(left: &str, right: &str) -> i32 {
    if let (Ok(a), Ok(b)) = (left.parse::<f64>(), right.parse::<f64>()) {
        return match a.partial_cmp(&b) {
            Some(std::cmp::Ordering::Less) => -1,
            Some(std::cmp::Ordering::Equal) => 0,
            Some(std::cmp::Ordering::Greater) => 1,
            None => 0,
        };
    }
    match left.cmp(right) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

fn is_truthy(v: &str) -> bool {
    if is_nullish(v) {
        return false;
    }
    if let Ok(n) = v.parse::<f64>() {
        return n != 0.0;
    }
    let prefix: String = v
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    prefix.parse::<f64>().map(|n| n != 0.0).unwrap_or(false)
}

fn eval_case(
    row: &Row,
    columns: &[String],
    operand: Option<&Expr>,
    conditions: &[Expr],
    results: &[Expr],
    else_result: Option<&Expr>,
    session: Option<&Session>,
) -> Result<String, ExecError> {
    if conditions.len() != results.len() {
        return Err(ExecError::Message(
            "CASE WHEN/THEN branch count mismatch".into(),
        ));
    }
    for (cond, result) in conditions.iter().zip(results.iter()) {
        let matched = if let Some(opnd) = operand {
            let left = eval_expr(row, columns, opnd, session)?;
            let right = eval_expr(row, columns, cond, session)?;
            if is_nullish(&left) || is_nullish(&right) {
                false
            } else {
                compare_for_expr(&left, &right) == 0
            }
        } else {
            is_truthy(&eval_expr(row, columns, cond, session)?)
        };
        if matched {
            return eval_expr(row, columns, result, session);
        }
    }
    match else_result {
        Some(e) => eval_expr(row, columns, e, session),
        None => Ok(String::new()),
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
        "DATABASE" | "SCHEMA" => {
            require_no_args(func)?;
            let db = session.map(|s| s.database.as_str()).unwrap_or("rusql");
            Ok(db.to_string())
        }
        "USER" | "CURRENT_USER" | "SESSION_USER" => {
            require_no_args(func)?;
            Ok(session_user_host(session))
        }
        "VERSION" => {
            require_no_args(func)?;
            Ok(SERVER_VERSION.to_string())
        }
        "IF" => eval_if(row, columns, func, session),
        other => Err(ExecError::Message(format!("unsupported function: {other}"))),
    }
}

fn eval_if(
    row: &Row,
    columns: &[String],
    func: &Function,
    session: Option<&Session>,
) -> Result<String, ExecError> {
    let args = function_args(row, columns, func, session)?;
    if args.len() != 3 {
        return Err(ExecError::Message("IF requires three arguments".into()));
    }
    if is_truthy(&args[0]) {
        Ok(args[1].clone())
    } else {
        Ok(args[2].clone())
    }
}

/// Matches handshake `server_version` (MySQL 8.0-compatible).
pub(crate) const SERVER_VERSION: &str = "8.0.33-rusql";

fn require_no_args(func: &Function) -> Result<(), ExecError> {
    match &func.args {
        FunctionArguments::None => Ok(()),
        FunctionArguments::List(list) if list.args.is_empty() => Ok(()),
        FunctionArguments::List(_) => Err(ExecError::Message(format!(
            "incorrect parameter count for function {}",
            func.name
        ))),
        FunctionArguments::Subquery(_) => Err(ExecError::Message(format!(
            "incorrect parameter count for function {}",
            func.name
        ))),
    }
}

fn session_user_host(session: Option<&Session>) -> String {
    match session {
        Some(s) => {
            let host = if s.host.is_empty() {
                "%"
            } else {
                s.host.as_str()
            };
            format!("{}@{}", s.user, host)
        }
        None => "root@%".into(),
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
    use rusql_core::Session;
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

    fn eval_sql_session(sql: &str, session: &Session) -> String {
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
        eval_expr(&vec![], &[], expr, Some(session)).unwrap()
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

    #[test]
    fn session_info_functions_with_session() {
        let mut session = Session::new(1, "app");
        session.host = "localhost".into();
        session.database = "app_db".into();
        assert_eq!(eval_sql_session("SELECT DATABASE()", &session), "app_db");
        assert_eq!(eval_sql_session("SELECT SCHEMA()", &session), "app_db");
        assert_eq!(eval_sql_session("SELECT USER()", &session), "app@localhost");
        assert_eq!(
            eval_sql_session("SELECT CURRENT_USER()", &session),
            "app@localhost"
        );
        assert_eq!(
            eval_sql_session("SELECT VERSION()", &session),
            SERVER_VERSION
        );
        assert!(SERVER_VERSION.contains("8.0"));
    }

    #[test]
    fn case_searched_and_simple() {
        assert_eq!(
            eval_sql(
                "SELECT CASE WHEN id = 1 THEN 'one' WHEN id = 2 THEN 'two' ELSE 'other' END FROM t",
                vec!["1".into()],
                &["id"]
            ),
            "one"
        );
        assert_eq!(
            eval_sql(
                "SELECT CASE WHEN id = 1 THEN 'one' WHEN id = 2 THEN 'two' ELSE 'other' END FROM t",
                vec!["2".into()],
                &["id"]
            ),
            "two"
        );
        assert_eq!(
            eval_sql(
                "SELECT CASE WHEN id = 1 THEN 'one' WHEN id = 2 THEN 'two' ELSE 'other' END FROM t",
                vec!["9".into()],
                &["id"]
            ),
            "other"
        );
        assert_eq!(
            eval_sql(
                "SELECT CASE name WHEN 'a' THEN 1 WHEN 'b' THEN 2 ELSE 0 END FROM t",
                vec!["b".into()],
                &["name"]
            ),
            "2"
        );
    }

    #[test]
    fn if_builtin() {
        assert_eq!(
            eval_sql(
                "SELECT IF(id > 0, 'yes', 'no') FROM t",
                vec!["3".into()],
                &["id"]
            ),
            "yes"
        );
        assert_eq!(
            eval_sql(
                "SELECT IF(id > 0, 'yes', 'no') FROM t",
                vec!["0".into()],
                &["id"]
            ),
            "no"
        );
        assert_eq!(
            eval_sql("SELECT IF(0, 'a', 'b') FROM t", vec!["1".into()], &["id"]),
            "b"
        );
    }
}

//! MySQL `@@` session/system variable stubs (M77 + M79), `SHOW VARIABLES` (M80),
//! `SET @@` overlays (M81), and `SET NAMES` / `@foo` (M82).
//!
//! Documented stub set for client/ORM probes. The full MySQL 8.0
//! `SHOW VARIABLES` catalog (~500 names) is out of scope.

use crate::{ExecError, QueryResult};
use rusql_core::Session;
use rusql_storage::Row;
use sqlparser::ast::{
    Expr, Ident, ObjectName, OneOrManyWithParens, ShowStatementFilter, Statement, Value,
};
use std::collections::HashMap;

/// Matches handshake `server_version` and `VERSION()` (MySQL 8.0-compatible).
pub(crate) const SERVER_VERSION: &str = "8.0.33-rusql";

/// Stable `@@version_comment` stub (same string historically returned by the identifier special-case).
pub(crate) const VERSION_COMMENT: &str = "8.0.33-rusql";

/// MySQL 8.0-like `sql_mode` stub. rusql does not enforce these modes.
pub(crate) const SQL_MODE_STUB: &str = "ONLY_FULL_GROUP_BY,STRICT_TRANS_TABLES,NO_ZERO_IN_DATE,NO_ZERO_DATE,ERROR_FOR_DIVISION_BY_ZERO,NO_ENGINE_SUBSTITUTION";

/// `@@collation_connection` stub (MySQL 8.0 default; rusql also supports `utf8mb4_unicode_ci`).
pub(crate) const COLLATION_CONNECTION: &str = "utf8mb4_0900_ai_ci";

/// `@@auto_increment_increment` stub (MySQL default; rusql does not honor a custom increment).
pub(crate) const AUTO_INCREMENT_INCREMENT: &str = "1";

/// `@@time_zone` stub (`SYSTEM` = follow `@@system_time_zone`; not a live TZ offset).
pub(crate) const TIME_ZONE: &str = "SYSTEM";

/// `@@system_time_zone` stub (fixed UTC; not the host TZ).
pub(crate) const SYSTEM_TIME_ZONE: &str = "UTC";

/// `@@transaction_isolation` / `@@tx_isolation` stub (rusql snapshot isolation).
pub(crate) const TRANSACTION_ISOLATION: &str = "REPEATABLE-READ";

/// `@@max_allowed_packet` stub (64 MiB; protocol length is not enforced from this value).
pub(crate) const MAX_ALLOWED_PACKET: &str = "67108864";

/// `@@license` stub (Community-style label; not a license check).
pub(crate) const LICENSE: &str = "GPL";

const CHARSET: &str = "utf8mb4";
const SCOPES: &[&str] = &["session", "local", "global"];

/// Documented M77+M79 stub names (alphabetical, as `SHOW VARIABLES` lists them).
const STUB_NAMES: &[&str] = &[
    "auto_increment_increment",
    "autocommit",
    "character_set_client",
    "character_set_connection",
    "character_set_results",
    "character_set_server",
    "collation_connection",
    "license",
    "max_allowed_packet",
    "sql_mode",
    "system_time_zone",
    "time_zone",
    "transaction_isolation",
    "tx_isolation",
    "version",
    "version_comment",
];

/// Names that reject `SET` (MySQL-like read-only stubs).
const READONLY_NAMES: &[&str] = &["license", "system_time_zone", "version", "version_comment"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum SysVarScope {
    Session,
    Global,
}

/// True when `expr` is a `@@` / `@@session.` system-variable reference.
pub(crate) fn is_session_var_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Identifier(id) => id.value.starts_with("@@"),
        Expr::CompoundIdentifier(parts) => {
            parts.first().is_some_and(|id| id.value.starts_with("@@"))
        }
        _ => false,
    }
}

/// True when `expr` is a user variable `@foo` (not `@@sysvar`).
pub(crate) fn is_user_var_expr(expr: &Expr) -> bool {
    user_var_name(expr).is_some()
}

/// Result-set column name for a `@@` reference (`@@version`, `@@session.autocommit`).
pub(crate) fn session_var_output_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(id) if id.value.starts_with("@@") => Some(id.value.clone()),
        Expr::CompoundIdentifier(parts)
            if parts.first().is_some_and(|id| id.value.starts_with("@@")) =>
        {
            Some(
                parts
                    .iter()
                    .map(|p| p.value.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
            )
        }
        _ => None,
    }
}

/// Evaluates a `@@` reference. Returns `Ok(None)` when `expr` is not a sysvar.
/// `@@global.` ignores the session overlay; `@@` / `@@session.` / `@@local.` use it.
pub(crate) fn eval_session_var(
    expr: &Expr,
    overlay: Option<&HashMap<String, String>>,
) -> Result<Option<String>, ExecError> {
    let Some((scope, name)) = sysvar_ref(expr) else {
        return Ok(None);
    };
    let overlay = overlay.filter(|_| scope != SysVarScope::Global);
    Ok(Some(lookup_session_var(&name, overlay)?))
}

fn user_var_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(id) if id.value.starts_with('@') && !id.value.starts_with("@@") => {
            Some(id.value.trim_start_matches('@').to_ascii_lowercase())
        }
        _ => None,
    }
}

/// Evaluates `@foo`. Unset variables yield an empty cell (NULL).
pub(crate) fn eval_user_var(
    expr: &Expr,
    overlay: Option<&HashMap<String, String>>,
) -> Option<String> {
    let name = user_var_name(expr)?;
    Some(
        overlay
            .and_then(|m| m.get(&name).cloned())
            .unwrap_or_default(),
    )
}

fn sysvar_ref(expr: &Expr) -> Option<(SysVarScope, String)> {
    match expr {
        Expr::Identifier(id) => {
            let rest = strip_at_at(&id.value)?;
            Some(scope_and_name(&rest, &[]))
        }
        Expr::CompoundIdentifier(parts) => sysvar_ref_from_parts(parts),
        _ => None,
    }
}

fn strip_at_at(raw: &str) -> Option<String> {
    raw.strip_prefix("@@").map(str::to_string)
}

fn sysvar_ref_from_parts(parts: &[Ident]) -> Option<(SysVarScope, String)> {
    let first = parts.first()?;
    let rest = strip_at_at(&first.value)?;
    Some(scope_and_name(&rest, &parts[1..]))
}

fn scope_and_name(first: &str, rest: &[Ident]) -> (SysVarScope, String) {
    if rest.is_empty() {
        return (SysVarScope::Session, first.to_string());
    }
    if SCOPES.iter().any(|s| first.eq_ignore_ascii_case(s)) {
        let scope = if first.eq_ignore_ascii_case("global") {
            SysVarScope::Global
        } else {
            SysVarScope::Session
        };
        let name = rest
            .iter()
            .map(|p| p.value.as_str())
            .collect::<Vec<_>>()
            .join(".");
        return (scope, name);
    }
    let mut name = first.to_string();
    for part in rest {
        name.push('.');
        name.push_str(&part.value);
    }
    (SysVarScope::Session, name)
}

fn lookup_session_var(
    name: &str,
    overlay: Option<&HashMap<String, String>>,
) -> Result<String, ExecError> {
    let key = name.to_ascii_lowercase();
    if let Some(map) = overlay {
        if let Some(v) = map.get(&key) {
            return Ok(v.clone());
        }
    }
    default_session_var(&key, name)
}

fn default_session_var(key: &str, display_name: &str) -> Result<String, ExecError> {
    match key {
        "version" => Ok(SERVER_VERSION.to_string()),
        "version_comment" => Ok(VERSION_COMMENT.to_string()),
        "autocommit" => Ok("1".into()),
        "sql_mode" => Ok(SQL_MODE_STUB.into()),
        "character_set_client"
        | "character_set_connection"
        | "character_set_results"
        | "character_set_server" => Ok(CHARSET.into()),
        "collation_connection" => Ok(COLLATION_CONNECTION.into()),
        "auto_increment_increment" => Ok(AUTO_INCREMENT_INCREMENT.into()),
        "time_zone" => Ok(TIME_ZONE.into()),
        "system_time_zone" => Ok(SYSTEM_TIME_ZONE.into()),
        "transaction_isolation" | "tx_isolation" => Ok(TRANSACTION_ISOLATION.into()),
        "max_allowed_packet" => Ok(MAX_ALLOWED_PACKET.into()),
        "license" => Ok(LICENSE.into()),
        _ => Err(ExecError::Mysql {
            code: 1193,
            message: rusql_i18n::messages::sql_unknown_system_variable(display_name),
        }),
    }
}

/// `SHOW [SESSION|GLOBAL] VARIABLES [LIKE …]` over the documented stub catalog.
/// Session lists apply the connection overlay; global lists use documented defaults.
pub(crate) fn show_variables(
    filter: Option<&ShowStatementFilter>,
    overlay: &HashMap<String, String>,
    global: bool,
) -> QueryResult {
    let pattern = filter.and_then(|f| match f {
        ShowStatementFilter::Like(p)
        | ShowStatementFilter::ILike(p)
        | ShowStatementFilter::NoKeyword(p) => Some(p.as_str()),
        ShowStatementFilter::Where(_) => None,
    });
    let overlay = if global { None } else { Some(overlay) };
    let mut rows: Vec<Row> = STUB_NAMES
        .iter()
        .filter_map(|name| {
            let value = lookup_session_var(name, overlay).ok()?;
            if let Some(pat) = pattern {
                if !like_ci(name, pat) {
                    return None;
                }
            }
            Some(vec![(*name).to_string(), value])
        })
        .collect();
    rows.sort_by(|a, b| a[0].cmp(&b[0]));
    QueryResult::Rows {
        columns: vec!["Variable_name".into(), "Value".into()],
        rows,
    }
}

/// `SET @@` / `SET SESSION` / `SET NAMES` / `SET @foo`.
pub(crate) fn execute_set_statement(
    session: &mut Session,
    stmt: &Statement,
) -> Result<QueryResult, ExecError> {
    match stmt {
        Statement::SetVariable {
            variables, value, ..
        } => apply_set_variable(session, variables, value),
        Statement::SetNames {
            charset_name,
            collation_name,
        } => apply_set_names(session, charset_name, collation_name.as_deref()),
        Statement::SetNamesDefault {} => apply_set_names_default(session),
        _ => Err(ExecError::Message(
            rusql_i18n::messages::sql_set_multi_assign_unsupported(),
        )),
    }
}

fn apply_set_names(
    session: &mut Session,
    charset: &str,
    collation: Option<&str>,
) -> Result<QueryResult, ExecError> {
    let charset = charset.to_ascii_lowercase();
    for key in [
        "character_set_client",
        "character_set_connection",
        "character_set_results",
    ] {
        session
            .session_vars
            .insert(key.to_string(), charset.clone());
    }
    if let Some(collation) = collation {
        session
            .session_vars
            .insert("collation_connection".into(), collation.to_string());
    } else if charset == "utf8mb4" {
        session
            .session_vars
            .insert("collation_connection".into(), COLLATION_CONNECTION.into());
    }
    Ok(QueryResult::Ok { rows_affected: 0 })
}

fn apply_set_names_default(session: &mut Session) -> Result<QueryResult, ExecError> {
    for key in [
        "character_set_client",
        "character_set_connection",
        "character_set_results",
        "collation_connection",
    ] {
        session.session_vars.remove(key);
    }
    Ok(QueryResult::Ok { rows_affected: 0 })
}

fn apply_set_variable(
    session: &mut Session,
    variables: &OneOrManyWithParens<ObjectName>,
    values: &[Expr],
) -> Result<QueryResult, ExecError> {
    let vars: &[ObjectName] = variables.as_ref();
    if vars.len() != 1 || values.len() != 1 {
        return Err(ExecError::Message(
            rusql_i18n::messages::sql_set_multi_assign_unsupported(),
        ));
    }
    if let Some(ident) = vars[0].0.first() {
        let raw = ident.value.as_str();
        if raw.starts_with('@') && !raw.starts_with("@@") {
            return apply_user_var_set(session, raw, &values[0]);
        }
    }
    let (scope, name) = parse_set_target(&vars[0])?;
    if scope == SysVarScope::Global {
        return Err(ExecError::Mysql {
            code: 1229,
            message: rusql_i18n::messages::sql_set_global_rejected(&name),
        });
    }
    let key = name.to_ascii_lowercase();
    default_session_var(&key, &name)?;
    if READONLY_NAMES.contains(&key.as_str()) {
        return Err(ExecError::Mysql {
            code: 1238,
            message: rusql_i18n::messages::sql_variable_is_readonly(&key),
        });
    }
    let raw = expr_to_set_value(&values[0])?;
    let stored = normalize_assignment(&key, &raw)?;
    for overlay_key in overlay_keys(&key) {
        session.session_vars.insert(overlay_key, stored.clone());
    }
    Ok(QueryResult::Ok { rows_affected: 0 })
}

fn apply_user_var_set(
    session: &mut Session,
    raw_name: &str,
    value: &Expr,
) -> Result<QueryResult, ExecError> {
    let key = raw_name.trim_start_matches('@').to_ascii_lowercase();
    let stored = expr_to_set_value(value)?;
    session.user_vars.insert(key, stored);
    Ok(QueryResult::Ok { rows_affected: 0 })
}

fn parse_set_target(name: &ObjectName) -> Result<(SysVarScope, String), ExecError> {
    let parts = &name.0;
    let first = parts.first().ok_or_else(|| ExecError::Mysql {
        code: 1193,
        message: rusql_i18n::messages::sql_unknown_system_variable(""),
    })?;
    let raw = first.value.as_str();
    if let Some(stripped) = strip_at_at(raw) {
        return Ok(scope_and_name(&stripped, &parts[1..]));
    }
    if parts.len() > 1 && SCOPES.iter().any(|s| raw.eq_ignore_ascii_case(s)) {
        return Ok(scope_and_name(raw, &parts[1..]));
    }
    let joined = parts
        .iter()
        .map(|p| p.value.as_str())
        .collect::<Vec<_>>()
        .join(".");
    Ok((SysVarScope::Session, joined))
}

fn overlay_keys(name: &str) -> Vec<String> {
    match name {
        "transaction_isolation" | "tx_isolation" => {
            vec!["transaction_isolation".into(), "tx_isolation".into()]
        }
        other => vec![other.to_string()],
    }
}

fn expr_to_set_value(expr: &Expr) -> Result<String, ExecError> {
    match expr {
        Expr::Value(Value::Null) => Ok(String::new()),
        Expr::Value(Value::Number(n, _)) => Ok(n.clone()),
        Expr::Value(Value::SingleQuotedString(s) | Value::DoubleQuotedString(s)) => Ok(s.clone()),
        Expr::Value(Value::Boolean(b)) => Ok(if *b { "1".into() } else { "0".into() }),
        Expr::Identifier(id) => Ok(id.value.clone()),
        Expr::UnaryOp {
            op: sqlparser::ast::UnaryOperator::Minus,
            expr: inner,
        } => {
            let v = expr_to_set_value(inner)?;
            Ok(format!("-{v}"))
        }
        other => Err(ExecError::Mysql {
            code: 1231,
            message: rusql_i18n::messages::sql_wrong_value_for_var("SET", &format!("{other}")),
        }),
    }
}

fn normalize_assignment(name: &str, raw: &str) -> Result<String, ExecError> {
    if name == "autocommit" {
        return match raw.trim().to_ascii_lowercase().as_str() {
            "0" | "off" | "false" | "no" => Ok("0".into()),
            "1" | "on" | "true" | "yes" => Ok("1".into()),
            _ => Err(ExecError::Mysql {
                code: 1231,
                message: rusql_i18n::messages::sql_wrong_value_for_var(name, raw),
            }),
        };
    }
    Ok(raw.to_string())
}

fn like_ci(name: &str, pattern: &str) -> bool {
    like_bytes(
        name.to_ascii_lowercase().as_bytes(),
        pattern.to_ascii_lowercase().as_bytes(),
    )
}

fn like_bytes(name: &[u8], pattern: &[u8]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some((b'%', rest)) => {
            like_bytes(name, rest) || (!name.is_empty() && like_bytes(&name[1..], pattern))
        }
        Some((b'_', rest)) => !name.is_empty() && like_bytes(&name[1..], rest),
        Some((ch, rest)) => name.first() == Some(ch) && like_bytes(&name[1..], rest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_sql::parse;
    use sqlparser::ast::{SelectItem, SetExpr, Statement};

    fn first_expr(sql: &str) -> Expr {
        let stmt = parse(sql).unwrap().into_iter().next().unwrap();
        let Statement::Query(q) = stmt else {
            panic!("expected query");
        };
        let SetExpr::Select(select) = q.body.as_ref() else {
            panic!("expected select");
        };
        match &select.projection[0] {
            SelectItem::UnnamedExpr(expr) => expr.clone(),
            other => panic!("expected unnamed expr, got {other:?}"),
        }
    }

    fn eval_sql(sql: &str) -> String {
        eval_session_var(&first_expr(sql), None)
            .unwrap()
            .expect("expected sysvar")
    }

    fn eval_sql_overlay(sql: &str, overlay: &HashMap<String, String>) -> String {
        eval_session_var(&first_expr(sql), Some(overlay))
            .unwrap()
            .expect("expected sysvar")
    }

    #[test]
    fn session_var_stub_values() {
        assert_eq!(eval_sql("SELECT @@version"), SERVER_VERSION);
        assert!(SERVER_VERSION.contains("8.0"));
        assert_eq!(eval_sql("SELECT @@version_comment"), VERSION_COMMENT);
        assert_eq!(eval_sql("SELECT @@autocommit"), "1");
        assert_eq!(eval_sql("SELECT @@sql_mode"), SQL_MODE_STUB);
        assert_eq!(eval_sql("SELECT @@character_set_client"), CHARSET);
        assert_eq!(eval_sql("SELECT @@character_set_connection"), CHARSET);
        assert_eq!(eval_sql("SELECT @@character_set_results"), CHARSET);
        assert_eq!(eval_sql("SELECT @@character_set_server"), CHARSET);
        assert_eq!(
            eval_sql("SELECT @@collation_connection"),
            COLLATION_CONNECTION
        );
        assert_eq!(
            eval_sql("SELECT @@auto_increment_increment"),
            AUTO_INCREMENT_INCREMENT
        );
        assert_eq!(eval_sql("SELECT @@time_zone"), TIME_ZONE);
        assert_eq!(eval_sql("SELECT @@system_time_zone"), SYSTEM_TIME_ZONE);
        assert_eq!(
            eval_sql("SELECT @@transaction_isolation"),
            TRANSACTION_ISOLATION
        );
        assert_eq!(eval_sql("SELECT @@tx_isolation"), TRANSACTION_ISOLATION);
        assert_eq!(eval_sql("SELECT @@max_allowed_packet"), MAX_ALLOWED_PACKET);
        assert_eq!(eval_sql("SELECT @@license"), LICENSE);
    }

    #[test]
    fn session_var_session_prefix_equivalent() {
        assert_eq!(
            eval_sql("SELECT @@session.version"),
            eval_sql("SELECT @@version")
        );
        assert_eq!(
            eval_sql("SELECT @@session.autocommit"),
            eval_sql("SELECT @@autocommit")
        );
        assert_eq!(
            eval_sql("SELECT @@SESSION.character_set_client"),
            eval_sql("SELECT @@character_set_client")
        );
        assert_eq!(
            eval_sql("SELECT @@local.sql_mode"),
            eval_sql("SELECT @@sql_mode")
        );
        assert_eq!(
            eval_sql("SELECT @@session.auto_increment_increment"),
            eval_sql("SELECT @@auto_increment_increment")
        );
        assert_eq!(
            eval_sql("SELECT @@session.time_zone"),
            eval_sql("SELECT @@time_zone")
        );
        assert_eq!(
            eval_sql("SELECT @@session.transaction_isolation"),
            eval_sql("SELECT @@transaction_isolation")
        );
        assert_eq!(
            eval_sql("SELECT @@session.tx_isolation"),
            eval_sql("SELECT @@tx_isolation")
        );
        assert_eq!(
            eval_sql("SELECT @@session.max_allowed_packet"),
            eval_sql("SELECT @@max_allowed_packet")
        );
        assert_eq!(
            eval_sql("SELECT @@session.license"),
            eval_sql("SELECT @@license")
        );
        assert_eq!(
            eval_sql("SELECT @@session.system_time_zone"),
            eval_sql("SELECT @@system_time_zone")
        );
    }

    #[test]
    fn session_var_unknown_errno_1193() {
        match eval_session_var(&first_expr("SELECT @@not_a_real_var"), None) {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, 1193);
                assert!(message.contains("not_a_real_var"));
            }
            other => panic!("expected errno 1193, got {other:?}"),
        }
        match eval_session_var(&first_expr("SELECT @@session.not_a_real_var"), None) {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, 1193);
                assert!(message.contains("not_a_real_var"));
            }
            other => panic!("expected errno 1193 for @@session., got {other:?}"),
        }
    }

    #[test]
    fn session_var_output_names() {
        let version = first_expr("SELECT @@version");
        assert_eq!(
            session_var_output_name(&version).as_deref(),
            Some("@@version")
        );
        let scoped = first_expr("SELECT @@session.autocommit");
        assert_eq!(
            session_var_output_name(&scoped).as_deref(),
            Some("@@session.autocommit")
        );
    }

    #[test]
    fn show_variables_catalog_and_like() {
        let overlay = HashMap::new();
        match show_variables(None, &overlay, false) {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec!["Variable_name".to_string(), "Value".to_string()]
                );
                assert_eq!(rows.len(), STUB_NAMES.len());
                assert!(rows.windows(2).all(|w| w[0][0] <= w[1][0]));
                for name in STUB_NAMES {
                    let row = rows.iter().find(|r| r[0] == *name).unwrap();
                    assert_eq!(row[1], lookup_session_var(name, None).unwrap());
                }
            }
            other => panic!("expected SHOW VARIABLES rows, got {other:?}"),
        }

        let like = ShowStatementFilter::Like("auto_increment%".into());
        match show_variables(Some(&like), &overlay, false) {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec![
                        "auto_increment_increment".to_string(),
                        AUTO_INCREMENT_INCREMENT.to_string()
                    ]]
                );
            }
            other => panic!("expected LIKE rows, got {other:?}"),
        }

        let miss = ShowStatementFilter::Like("not_a_real_var%".into());
        match show_variables(Some(&miss), &overlay, false) {
            QueryResult::Rows { rows, .. } => {
                assert!(rows.is_empty());
            }
            other => panic!("expected empty LIKE rows, got {other:?}"),
        }
    }

    #[test]
    fn set_session_var_overlay_and_global_unaffected() {
        let mut overlay = HashMap::new();
        overlay.insert("autocommit".into(), "0".into());
        assert_eq!(eval_sql_overlay("SELECT @@autocommit", &overlay), "0");
        assert_eq!(
            eval_sql_overlay("SELECT @@session.autocommit", &overlay),
            "0"
        );
        assert_eq!(
            eval_sql_overlay("SELECT @@global.autocommit", &overlay),
            "1"
        );

        match show_variables(None, &overlay, false) {
            QueryResult::Rows { rows, .. } => {
                let row = rows.iter().find(|r| r[0] == "autocommit").unwrap();
                assert_eq!(row[1], "0");
            }
            other => panic!("expected session SHOW overlay, got {other:?}"),
        }
        match show_variables(None, &overlay, true) {
            QueryResult::Rows { rows, .. } => {
                let row = rows.iter().find(|r| r[0] == "autocommit").unwrap();
                assert_eq!(row[1], "1");
            }
            other => panic!("expected global SHOW defaults, got {other:?}"),
        }
    }

    #[test]
    fn set_session_var_execute_autocommit_and_reset() {
        let mut session = Session::new(1, "root");
        let set0 = parse("SET @@autocommit = 0")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        execute_set_statement(&mut session, &set0).unwrap();
        assert_eq!(
            eval_session_var(
                &first_expr("SELECT @@autocommit"),
                Some(&session.session_vars)
            )
            .unwrap()
            .unwrap(),
            "0"
        );

        let set_session = parse("SET SESSION autocommit = 1")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        execute_set_statement(&mut session, &set_session).unwrap();
        assert_eq!(
            eval_session_var(
                &first_expr("SELECT @@session.autocommit"),
                Some(&session.session_vars)
            )
            .unwrap()
            .unwrap(),
            "1"
        );

        let set_scoped = parse("SET @@session.autocommit = 0")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        execute_set_statement(&mut session, &set_scoped).unwrap();
        session.clear_session_vars();
        assert_eq!(
            eval_session_var(
                &first_expr("SELECT @@autocommit"),
                Some(&session.session_vars)
            )
            .unwrap()
            .unwrap(),
            "1"
        );
    }

    #[test]
    fn set_session_var_rejects_global_readonly_unknown() {
        let mut session = Session::new(1, "root");
        let global = parse("SET GLOBAL autocommit = 0")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match execute_set_statement(&mut session, &global) {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, 1229);
                assert!(message.contains("autocommit"));
            }
            other => panic!("expected errno 1229, got {other:?}"),
        }

        let ro = parse("SET @@version = 'nope'")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match execute_set_statement(&mut session, &ro) {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, 1238);
                assert!(message.contains("version"));
            }
            other => panic!("expected errno 1238, got {other:?}"),
        }

        let unknown = parse("SET @@not_a_real_var = 1")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        match execute_set_statement(&mut session, &unknown) {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, 1193);
                assert!(message.contains("not_a_real_var"));
            }
            other => panic!("expected errno 1193, got {other:?}"),
        }

        let names = parse("SET NAMES utf8mb4")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        execute_set_statement(&mut session, &names).unwrap();
        assert_eq!(
            session
                .session_vars
                .get("character_set_client")
                .map(String::as_str),
            Some("utf8mb4")
        );
    }
}

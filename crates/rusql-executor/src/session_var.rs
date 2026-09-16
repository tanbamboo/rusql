//! MySQL `@@` session/system variable stubs (M77 + M79).
//!
//! Documented stub set for client/ORM probes. `SET @@` and the full
//! `SHOW VARIABLES` catalog are out of scope.

use crate::ExecError;
use sqlparser::ast::{Expr, Ident};

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
pub(crate) fn eval_session_var(expr: &Expr) -> Result<Option<String>, ExecError> {
    let Some(name) = sysvar_name(expr) else {
        return Ok(None);
    };
    Ok(Some(lookup_session_var(&name)?))
}

fn sysvar_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(id) => strip_at_at(&id.value),
        Expr::CompoundIdentifier(parts) => sysvar_name_from_parts(parts),
        _ => None,
    }
}

fn strip_at_at(raw: &str) -> Option<String> {
    raw.strip_prefix("@@").map(str::to_string)
}

fn sysvar_name_from_parts(parts: &[Ident]) -> Option<String> {
    let first = parts.first()?;
    let rest = strip_at_at(&first.value)?;
    if parts.len() == 1 {
        return Some(rest);
    }
    if SCOPES.iter().any(|s| rest.eq_ignore_ascii_case(s)) {
        return Some(
            parts[1..]
                .iter()
                .map(|p| p.value.as_str())
                .collect::<Vec<_>>()
                .join("."),
        );
    }
    let mut name = rest;
    for part in &parts[1..] {
        name.push('.');
        name.push_str(&part.value);
    }
    Some(name)
}

fn lookup_session_var(name: &str) -> Result<String, ExecError> {
    match name.to_ascii_lowercase().as_str() {
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
            message: rusql_i18n::messages::sql_unknown_system_variable(name),
        }),
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
        eval_session_var(&first_expr(sql))
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
        match eval_session_var(&first_expr("SELECT @@not_a_real_var")) {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, 1193);
                assert!(message.contains("not_a_real_var"));
            }
            other => panic!("expected errno 1193, got {other:?}"),
        }
        match eval_session_var(&first_expr("SELECT @@session.not_a_real_var")) {
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
}

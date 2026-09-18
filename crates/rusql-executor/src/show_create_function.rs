//! Documented `SHOW CREATE FUNCTION` stubs (M96).
//!
//! `Create Function` is reconstructed from catalog `FunctionMeta`.
//! Parameter lists are empty stubs — not persisted on `CREATE FUNCTION`.
//! Other cells are stubs — not live DEFINER / sql_mode / charset catalogs.

use crate::info_schema::{DEFAULT_CHARSET, DEFAULT_COLLATION};
use crate::{ExecError, QueryResult};
use rusql_core::{FunctionMeta, Session};

pub(crate) const CREATE_FUNCTION_VIRTUAL_TABLE: &str = rusql_sql::CREATE_FUNCTION_VIRTUAL_TABLE;

/// MySQL 8.0 `SHOW CREATE FUNCTION` column order.
pub(crate) const COLUMNS: [&str; 6] = [
    "Function",
    "sql_mode",
    "Create Function",
    "character_set_client",
    "collation_connection",
    "Database Collation",
];

/// MySQL `ER_SP_DOES_NOT_EXIST`.
const ER_SP_DOES_NOT_EXIST: u16 = 1305;

/// `SHOW CREATE FUNCTION [db.]name` reconstructed from the session catalog.
pub(crate) fn show_create_function(
    session: &Session,
    database: Option<&str>,
    name: &str,
) -> Result<QueryResult, ExecError> {
    let db = database.unwrap_or(session.database.as_str());
    let meta = find_function(session, db, name).ok_or_else(|| ExecError::Mysql {
        code: ER_SP_DOES_NOT_EXIST,
        message: rusql_i18n::messages::function_not_found(name),
    })?;
    Ok(QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows: vec![function_row(meta)],
    })
}

fn find_function<'a>(session: &'a Session, schema: &str, name: &str) -> Option<&'a FunctionMeta> {
    session.catalog.get_function(schema, name).or_else(|| {
        session
            .catalog
            .iter_functions()
            .find(|f| f.schema.eq_ignore_ascii_case(schema) && f.name.eq_ignore_ascii_case(name))
    })
}

fn function_row(meta: &FunctionMeta) -> Vec<String> {
    vec![
        meta.name.clone(),
        String::new(),
        create_function_ddl(meta),
        DEFAULT_CHARSET.to_string(),
        DEFAULT_COLLATION.to_string(),
        DEFAULT_COLLATION.to_string(),
    ]
}

fn create_function_ddl(meta: &FunctionMeta) -> String {
    let name = meta.name.replace('`', "``");
    format!(
        "CREATE FUNCTION `{name}`() RETURNS {} BEGIN RETURN {}; END",
        meta.return_type, meta.return_expr
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::DEFAULT_SCHEMA;

    fn session_with_function(name: &str, return_type: &str, return_expr: &str) -> Session {
        let mut session = Session::new(1, "root");
        session.catalog.create_function(FunctionMeta {
            schema: DEFAULT_SCHEMA.into(),
            name: name.into(),
            return_type: return_type.into(),
            return_expr: return_expr.into(),
        });
        session
    }

    #[test]
    fn show_create_function_columns_reconstructed_ddl_and_unknown() {
        let session = session_with_function("f", "INT", "42");

        match show_create_function(&session, None, "f") {
            Ok(QueryResult::Rows { columns, rows }) => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "f");
                assert_eq!(rows[0][1], "");
                assert_eq!(
                    rows[0][2],
                    "CREATE FUNCTION `f`() RETURNS INT BEGIN RETURN 42; END"
                );
                assert!(!rows[0][2].contains("DEFINER"));
                assert_eq!(rows[0][3], DEFAULT_CHARSET);
                assert_eq!(rows[0][4], DEFAULT_COLLATION);
                assert_eq!(rows[0][5], DEFAULT_COLLATION);
            }
            other => panic!("expected rows, got {other:?}"),
        }

        match show_create_function(&session, None, "no_such_fn") {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, ER_SP_DOES_NOT_EXIST);
                assert!(message.contains("no_such_fn"));
            }
            other => panic!("expected errno 1305, got {other:?}"),
        }
    }
}

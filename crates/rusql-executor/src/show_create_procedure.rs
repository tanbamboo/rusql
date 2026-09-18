//! Documented `SHOW CREATE PROCEDURE` stubs (M95).
//!
//! `Create Procedure` is reconstructed from catalog `ProcedureMeta`.
//! Parameter lists are empty stubs — not persisted on `CREATE PROCEDURE`.
//! Other cells are stubs — not live DEFINER / sql_mode / charset catalogs.

use crate::info_schema::{DEFAULT_CHARSET, DEFAULT_COLLATION};
use crate::{ExecError, QueryResult};
use rusql_core::{ProcedureMeta, Session};

pub(crate) const CREATE_PROCEDURE_VIRTUAL_TABLE: &str = rusql_sql::CREATE_PROCEDURE_VIRTUAL_TABLE;

/// MySQL 8.0 `SHOW CREATE PROCEDURE` column order.
pub(crate) const COLUMNS: [&str; 6] = [
    "Procedure",
    "sql_mode",
    "Create Procedure",
    "character_set_client",
    "collation_connection",
    "Database Collation",
];

/// MySQL `ER_SP_DOES_NOT_EXIST`.
const ER_SP_DOES_NOT_EXIST: u16 = 1305;

/// `SHOW CREATE PROCEDURE [db.]name` reconstructed from the session catalog.
pub(crate) fn show_create_procedure(
    session: &Session,
    database: Option<&str>,
    name: &str,
) -> Result<QueryResult, ExecError> {
    let db = database.unwrap_or(session.database.as_str());
    let meta = find_procedure(session, db, name).ok_or_else(|| ExecError::Mysql {
        code: ER_SP_DOES_NOT_EXIST,
        message: rusql_i18n::messages::procedure_not_found(name),
    })?;
    Ok(QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows: vec![procedure_row(meta)],
    })
}

fn find_procedure<'a>(session: &'a Session, schema: &str, name: &str) -> Option<&'a ProcedureMeta> {
    session.catalog.get_procedure(schema, name).or_else(|| {
        session
            .catalog
            .iter_procedures()
            .find(|p| p.schema.eq_ignore_ascii_case(schema) && p.name.eq_ignore_ascii_case(name))
    })
}

fn procedure_row(meta: &ProcedureMeta) -> Vec<String> {
    vec![
        meta.name.clone(),
        String::new(),
        create_procedure_ddl(meta),
        DEFAULT_CHARSET.to_string(),
        DEFAULT_COLLATION.to_string(),
        DEFAULT_COLLATION.to_string(),
    ]
}

fn create_procedure_ddl(meta: &ProcedureMeta) -> String {
    let name = meta.name.replace('`', "``");
    let inner = if meta.body.is_empty() {
        String::new()
    } else {
        format!(" {};", meta.body.join("; "))
    };
    format!("CREATE PROCEDURE `{name}`() BEGIN{inner} END")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::DEFAULT_SCHEMA;

    fn session_with_procedure(name: &str, body: Vec<String>) -> Session {
        let mut session = Session::new(1, "root");
        session.catalog.create_procedure(ProcedureMeta {
            schema: DEFAULT_SCHEMA.into(),
            name: name.into(),
            body,
        });
        session
    }

    #[test]
    fn show_create_procedure_columns_reconstructed_ddl_and_unknown() {
        let session = session_with_procedure("p", vec!["INSERT INTO t VALUES (42)".into()]);

        match show_create_procedure(&session, None, "p") {
            Ok(QueryResult::Rows { columns, rows }) => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "p");
                assert_eq!(rows[0][1], "");
                assert_eq!(
                    rows[0][2],
                    "CREATE PROCEDURE `p`() BEGIN INSERT INTO t VALUES (42); END"
                );
                assert!(!rows[0][2].contains("DEFINER"));
                assert_eq!(rows[0][3], DEFAULT_CHARSET);
                assert_eq!(rows[0][4], DEFAULT_COLLATION);
                assert_eq!(rows[0][5], DEFAULT_COLLATION);
            }
            other => panic!("expected rows, got {other:?}"),
        }

        match show_create_procedure(&session, None, "no_such_proc") {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, ER_SP_DOES_NOT_EXIST);
                assert!(message.contains("no_such_proc"));
            }
            other => panic!("expected errno 1305, got {other:?}"),
        }
    }
}

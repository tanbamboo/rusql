//! `SHOW CREATE DATABASE` / `SHOW CREATE SCHEMA` from the live catalog (M91/M114).
//!
//! Columns match MySQL: `Database`, `Create Database`.

use crate::info_schema::{DEFAULT_CHARSET, DEFAULT_COLLATION};
use crate::{ExecError, QueryResult};
use rusql_storage::StorageEngine;

pub(crate) const CREATE_DATABASE_VIRTUAL_TABLE: &str = rusql_sql::CREATE_DATABASE_VIRTUAL_TABLE;

pub(crate) const COLUMNS: [&str; 2] = ["Database", "Create Database"];

/// `SHOW CREATE DATABASE db` / `SHOW CREATE SCHEMA db` from stored charset/collation.
pub(crate) fn show_create_database<E: StorageEngine>(
    engine: &E,
    database: &str,
) -> Result<QueryResult, ExecError> {
    if !engine.list_databases().iter().any(|d| d == database) {
        return Err(ExecError::Mysql {
            code: 1049,
            message: rusql_i18n::messages::storage_database_not_found(database),
        });
    }
    let (charset, collation) = engine
        .database_charset_collation(database)
        .unwrap_or_else(|| (DEFAULT_CHARSET.to_string(), DEFAULT_COLLATION.to_string()));
    let ident = database.replace('`', "``");
    let ddl = format!(
        "CREATE DATABASE `{ident}` /*!40100 DEFAULT CHARACTER SET {charset} COLLATE {collation} */"
    );
    Ok(QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows: vec![vec![database.to_string(), ddl]],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_storage::HeapEngine;
    use rusql_storage::StorageEngine;

    #[test]
    fn show_create_database_uses_live_catalog() {
        let mut engine = HeapEngine::new();
        match show_create_database(&engine, "rusql") {
            Ok(QueryResult::Rows { columns, rows }) => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "rusql");
                assert!(rows[0][1].contains("CREATE DATABASE `rusql`"));
                assert!(rows[0][1].contains(DEFAULT_CHARSET));
                assert!(rows[0][1].contains(DEFAULT_COLLATION));
            }
            other => panic!("expected rows, got {other:?}"),
        }

        engine
            .create_database_with_charset("gap_cs", Some("utf8mb4"), Some("utf8mb4_0900_ai_ci"))
            .unwrap();
        match show_create_database(&engine, "gap_cs") {
            Ok(QueryResult::Rows { rows, .. }) => {
                assert!(rows[0][1].contains("utf8mb4_0900_ai_ci"));
                assert!(rows[0][1].contains("utf8mb4"));
            }
            other => panic!("expected live collation, got {other:?}"),
        }

        match show_create_database(&engine, "no_such_db") {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, 1049);
                assert!(message.contains("no_such_db"));
            }
            other => panic!("expected errno 1049, got {other:?}"),
        }
    }
}

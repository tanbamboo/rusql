//! Documented `SHOW CREATE DATABASE` / `SHOW CREATE SCHEMA` stubs (M91).
//!
//! Constant charset/collation comments — not a live per-schema catalog.
//! Columns match MySQL: `Database`, `Create Database`.

use crate::info_schema::DEFAULT_COLLATION;
use crate::{ExecError, QueryResult};
use rusql_storage::StorageEngine;

pub(crate) const CREATE_DATABASE_VIRTUAL_TABLE: &str = rusql_sql::CREATE_DATABASE_VIRTUAL_TABLE;

pub(crate) const COLUMNS: [&str; 2] = ["Database", "Create Database"];

const STUB_CHARSET: &str = "utf8mb4";

/// `SHOW CREATE DATABASE db` / `SHOW CREATE SCHEMA db` over the documented stub DDL.
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
    let ident = database.replace('`', "``");
    let ddl = format!(
        "CREATE DATABASE `{ident}` /*!40100 DEFAULT CHARACTER SET {STUB_CHARSET} COLLATE {DEFAULT_COLLATION} */"
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

    #[test]
    fn show_create_database_columns_stub_ddl_and_unknown() {
        let engine = HeapEngine::new();
        match show_create_database(&engine, "rusql") {
            Ok(QueryResult::Rows { columns, rows }) => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "rusql");
                assert!(rows[0][1].contains("CREATE DATABASE `rusql`"));
                assert!(rows[0][1].contains(STUB_CHARSET));
                assert!(rows[0][1].contains(DEFAULT_COLLATION));
            }
            other => panic!("expected rows, got {other:?}"),
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

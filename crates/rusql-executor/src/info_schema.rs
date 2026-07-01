//! Virtual information_schema and DESCRIBE result helpers.

use rusql_core::{Session, TableMeta};
use rusql_storage::{Row, StorageEngine};

use crate::{ExecError, QueryResult};

pub const DEFAULT_SCHEMA: &str = "rusql";

const DESCRIBE_COLUMNS: [&str; 6] = ["Field", "Type", "Null", "Key", "Default", "Extra"];

const INFO_TABLES_COLUMNS: [&str; 3] = ["TABLE_SCHEMA", "TABLE_NAME", "TABLE_TYPE"];

const INFO_COLUMNS_COLUMNS: [&str; 6] = [
    "TABLE_SCHEMA",
    "TABLE_NAME",
    "COLUMN_NAME",
    "ORDINAL_POSITION",
    "COLUMN_TYPE",
    "IS_NULLABLE",
];

/// DESCRIBE / SHOW COLUMNS result for one table.
pub fn describe_table(meta: &TableMeta) -> QueryResult {
    let rows: Vec<Row> = meta
        .columns
        .iter()
        .map(|c| {
            vec![
                c.name.clone(),
                c.data_type.to_lowercase(),
                "YES".into(),
                "".into(),
                "NULL".into(),
                "".into(),
            ]
        })
        .collect();
    QueryResult::Rows {
        columns: DESCRIBE_COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows,
    }
}

pub fn describe_table_by_name(session: &Session, table: &str) -> Result<QueryResult, ExecError> {
    let meta =
        session.catalog.get_table(table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(table))
        })?;
    Ok(describe_table(&meta))
}

/// `SHOW CREATE TABLE` result (Table, Create Table).
pub fn show_create_table(meta: &TableMeta) -> QueryResult {
    let col_defs: Vec<String> = meta
        .columns
        .iter()
        .map(|c| format!("`{}` {}", c.name, c.data_type.to_uppercase()))
        .collect();
    let ddl = format!("CREATE TABLE `{}` ({})", meta.name, col_defs.join(", "));
    QueryResult::Rows {
        columns: vec!["Table".into(), "Create Table".into()],
        rows: vec![vec![meta.name.clone(), ddl]],
    }
}

pub fn show_create_table_by_name(session: &Session, table: &str) -> Result<QueryResult, ExecError> {
    let meta =
        session.catalog.get_table(table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(table))
        })?;
    Ok(show_create_table(&meta))
}

/// `SELECT * FROM information_schema.tables`
pub fn scan_information_schema_tables<E: StorageEngine>(engine: &E, schema: &str) -> QueryResult {
    let mut names = engine.table_names();
    names.sort();
    let rows: Vec<Row> = names
        .into_iter()
        .map(|t| vec![schema.into(), t, "BASE TABLE".into()])
        .collect();
    QueryResult::Rows {
        columns: INFO_TABLES_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    }
}

/// `SELECT * FROM information_schema.columns [WHERE table_name = '…']`
pub fn scan_information_schema_columns<E: StorageEngine>(
    engine: &E,
    session: &Session,
    table_filter: Option<&str>,
) -> Result<QueryResult, ExecError> {
    let mut names = engine.table_names();
    names.sort();
    let mut rows = Vec::new();
    for table in names {
        if table_filter.is_some_and(|f| !f.eq_ignore_ascii_case(&table)) {
            continue;
        }
        let meta = session.catalog.get_table(&table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(&table))
        })?;
        for (i, col) in meta.columns.iter().enumerate() {
            rows.push(vec![
                session.database.clone(),
                table.clone(),
                col.name.clone(),
                (i + 1).to_string(),
                col.data_type.to_lowercase(),
                "YES".into(),
            ]);
        }
    }
    Ok(QueryResult::Rows {
        columns: INFO_COLUMNS_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    })
}

pub fn is_information_schema_table(name: &str) -> Option<&'static str> {
    match name {
        "information_schema.tables" => Some("tables"),
        "information_schema.columns" => Some("columns"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::ColumnDef;
    use rusql_storage::HeapEngine;

    #[test]
    fn show_create_shape() {
        let meta = TableMeta {
            name: "t".into(),
            columns: vec![
                ColumnDef {
                    name: "id".into(),
                    data_type: "int".into(),
                },
                ColumnDef {
                    name: "name".into(),
                    data_type: "varchar(32)".into(),
                },
            ],
        };
        match show_create_table(&meta) {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec!["Table".to_string(), "Create Table".to_string()]
                );
                assert_eq!(rows[0][0], "t");
                assert!(rows[0][1].contains("CREATE TABLE `t`"));
                assert!(rows[0][1].contains("`id` INT"));
                assert!(rows[0][1].contains("`name` VARCHAR(32)"));
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn describe_shape() {
        let meta = TableMeta {
            name: "t".into(),
            columns: vec![ColumnDef {
                name: "id".into(),
                data_type: "INT".into(),
            }],
        };
        match describe_table(&meta) {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns[0], "Field");
                assert_eq!(rows[0][0], "id");
                assert_eq!(rows[0][1], "int");
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn info_schema_tables() {
        let mut eng = HeapEngine::new();
        eng.create_table(TableMeta {
            name: "a".into(),
            columns: vec![ColumnDef {
                name: "x".into(),
                data_type: "INT".into(),
            }],
        })
        .unwrap();
        match scan_information_schema_tables(&eng, DEFAULT_SCHEMA) {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec![
                        "rusql".to_string(),
                        "a".to_string(),
                        "BASE TABLE".to_string(),
                    ]]
                );
            }
            _ => panic!("expected rows"),
        }
    }
}

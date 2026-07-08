//! Virtual information_schema and DESCRIBE result helpers.

use rusql_core::{Session, TableMeta};
use rusql_storage::{Row, StorageEngine};

use crate::{ExecError, QueryResult};

pub const DEFAULT_SCHEMA: &str = "rusql";
pub const DEFAULT_CHARSET: &str = "utf8mb4";
pub const DEFAULT_COLLATION: &str = "utf8mb4_unicode_ci";

const DESCRIBE_COLUMNS: [&str; 6] = ["Field", "Type", "Null", "Key", "Default", "Extra"];

const INFO_TABLES_COLUMNS: [&str; 3] = ["TABLE_SCHEMA", "TABLE_NAME", "TABLE_TYPE"];

const INFO_COLUMNS_COLUMNS: [&str; 7] = [
    "TABLE_SCHEMA",
    "TABLE_NAME",
    "COLUMN_NAME",
    "ORDINAL_POSITION",
    "COLUMN_TYPE",
    "IS_NULLABLE",
    "COLUMN_COLLATION",
];

const INFO_SCHEMATA_COLUMNS: [&str; 3] = [
    "SCHEMA_NAME",
    "DEFAULT_CHARACTER_SET_NAME",
    "DEFAULT_COLLATION_NAME",
];

const INFO_STATISTICS_COLUMNS: [&str; 7] = [
    "TABLE_SCHEMA",
    "TABLE_NAME",
    "INDEX_NAME",
    "SEQ_IN_INDEX",
    "COLUMN_NAME",
    "NON_UNIQUE",
    "INDEX_TYPE",
];

const SHOW_INDEX_COLUMNS: [&str; 6] = [
    "Table",
    "Non_unique",
    "Key_name",
    "Seq_in_index",
    "Column_name",
    "Index_type",
];

pub const SHOW_INDEX_VIRTUAL_TABLE: &str = "__rusql_show_index";

/// DESCRIBE / SHOW COLUMNS result for one table.
pub fn describe_table(meta: &TableMeta) -> QueryResult {
    let rows: Vec<Row> = meta
        .columns
        .iter()
        .map(|c| {
            vec![
                c.name.clone(),
                c.data_type.to_lowercase(),
                if c.nullable {
                    "YES".into()
                } else {
                    "NO".into()
                },
                if c.primary_key {
                    "PRI".into()
                } else {
                    "".into()
                },
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
        .map(|c| {
            let mut def = format!("`{}` {}", c.name, c.data_type.to_uppercase());
            if !c.nullable {
                def.push_str(" NOT NULL");
            }
            if c.primary_key {
                def.push_str(" PRIMARY KEY");
            }
            def
        })
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
pub fn scan_information_schema_tables<E: StorageEngine>(
    engine: &E,
    session: &Session,
    schema: &str,
) -> QueryResult {
    let mut names: std::collections::HashSet<String> = engine.table_names().into_iter().collect();
    for view in session.catalog.view_names() {
        names.insert(view.clone());
    }
    let mut names: Vec<_> = names.into_iter().collect();
    names.sort();
    let rows: Vec<Row> = names
        .into_iter()
        .map(|t| {
            let kind = if session.catalog.is_view(&t) {
                "VIEW"
            } else {
                "BASE TABLE"
            };
            vec![schema.into(), t, kind.into()]
        })
        .collect();
    QueryResult::Rows {
        columns: INFO_TABLES_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    }
}

const INFO_VIEWS_COLUMNS: [&str; 3] = ["TABLE_SCHEMA", "TABLE_NAME", "VIEW_DEFINITION"];

/// `SELECT * FROM information_schema.VIEWS`
pub fn scan_information_schema_views(session: &Session) -> QueryResult {
    let mut names: Vec<_> = session.catalog.view_names().cloned().collect();
    names.sort();
    let rows: Vec<Row> = names
        .into_iter()
        .filter_map(|name| {
            session.catalog.get_view(&name).map(|view| {
                vec![
                    session.database.clone(),
                    view.name.clone(),
                    view.sql.clone(),
                ]
            })
        })
        .collect();
    QueryResult::Rows {
        columns: INFO_VIEWS_COLUMNS
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
                if col.nullable {
                    "YES".into()
                } else {
                    "NO".into()
                },
                DEFAULT_COLLATION.into(),
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

/// `SELECT * FROM information_schema.SCHEMATA`
pub fn scan_information_schema_schemata(schema: &str) -> QueryResult {
    QueryResult::Rows {
        columns: INFO_SCHEMATA_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows: vec![vec![
            schema.into(),
            DEFAULT_CHARSET.into(),
            DEFAULT_COLLATION.into(),
        ]],
    }
}

/// `SHOW INDEX FROM tbl` — MySQL-style index listing for one table.
pub fn show_index_for_table<E: StorageEngine>(
    engine: &E,
    session: &Session,
    table: &str,
) -> Result<QueryResult, ExecError> {
    let meta =
        session.catalog.get_table(table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(table))
        })?;
    let mut rows = Vec::new();
    for col in &meta.columns {
        if col.primary_key {
            rows.push(vec![
                table.into(),
                "0".into(),
                "PRIMARY".into(),
                "1".into(),
                col.name.clone(),
                "BTREE".into(),
            ]);
        }
    }
    for idx in engine.index_metas() {
        if idx.table.eq_ignore_ascii_case(table) {
            rows.push(vec![
                table.into(),
                "1".into(),
                idx.name.clone(),
                "1".into(),
                idx.column.clone(),
                "BTREE".into(),
            ]);
        }
    }
    rows.sort_by(|a, b| {
        (a[2].as_str(), a[4].as_str(), a[3].as_str()).cmp(&(
            b[2].as_str(),
            b[4].as_str(),
            b[3].as_str(),
        ))
    });
    Ok(QueryResult::Rows {
        columns: SHOW_INDEX_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    })
}

/// `SELECT * FROM information_schema.STATISTICS`
pub fn scan_information_schema_statistics<E: StorageEngine>(
    engine: &E,
    session: &Session,
) -> Result<QueryResult, ExecError> {
    let schema = session.database.clone();
    let mut rows = Vec::new();
    let mut tables = engine.table_names();
    tables.sort();
    for table in tables {
        let meta = session.catalog.get_table(&table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(&table))
        })?;
        for col in &meta.columns {
            if col.primary_key {
                rows.push(vec![
                    schema.clone(),
                    table.clone(),
                    "PRIMARY".into(),
                    "1".into(),
                    col.name.clone(),
                    "0".into(),
                    "BTREE".into(),
                ]);
            }
        }
    }
    for idx in engine.index_metas() {
        rows.push(vec![
            schema.clone(),
            idx.table.clone(),
            idx.name.clone(),
            "1".into(),
            idx.column.clone(),
            "1".into(),
            "BTREE".into(),
        ]);
    }
    rows.sort_by(|a, b| {
        (a[1].as_str(), a[2].as_str(), a[4].as_str(), a[3].as_str()).cmp(&(
            b[1].as_str(),
            b[2].as_str(),
            b[4].as_str(),
            b[3].as_str(),
        ))
    });
    Ok(QueryResult::Rows {
        columns: INFO_STATISTICS_COLUMNS
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
        "information_schema.SCHEMATA" | "information_schema.schemata" => Some("schemata"),
        "information_schema.STATISTICS" | "information_schema.statistics" => Some("statistics"),
        "information_schema.VIEWS" | "information_schema.views" => Some("views"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::ColumnDef;
    use rusql_storage::HeapEngine;

    #[test]
    fn describe_primary_key_metadata() {
        let meta = TableMeta {
            name: "pk_t".into(),
            columns: vec![
                ColumnDef {
                    name: "id".into(),
                    data_type: "INT".into(),
                    nullable: false,
                    primary_key: true,
                },
                ColumnDef {
                    name: "label".into(),
                    data_type: "VARCHAR(16)".into(),
                    nullable: false,
                    primary_key: false,
                },
            ],
        };
        match describe_table(&meta) {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows[0][2], "NO");
                assert_eq!(rows[0][3], "PRI");
                assert_eq!(rows[1][2], "NO");
                assert_eq!(rows[1][3], "");
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn show_create_shape() {
        let meta = TableMeta {
            name: "t".into(),
            columns: vec![
                ColumnDef::new("id", "int"),
                ColumnDef::new("name", "varchar(32)"),
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
            columns: vec![ColumnDef::new("id", "INT")],
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
    fn show_index_lists_primary_and_secondary() {
        let mut eng = HeapEngine::new();
        eng.create_table(TableMeta {
            name: "idx_t".into(),
            columns: vec![
                ColumnDef {
                    name: "id".into(),
                    data_type: "INT".into(),
                    nullable: false,
                    primary_key: true,
                },
                ColumnDef::new("name", "VARCHAR(32)"),
            ],
        })
        .unwrap();
        eng.create_index(rusql_core::IndexMeta {
            name: "idx_name".into(),
            table: "idx_t".into(),
            column: "name".into(),
        })
        .unwrap();

        let session = rusql_core::Session::new(1, "root");
        let mut session = session;
        session.catalog.create_table(TableMeta {
            name: "idx_t".into(),
            columns: vec![
                ColumnDef {
                    name: "id".into(),
                    data_type: "INT".into(),
                    nullable: false,
                    primary_key: true,
                },
                ColumnDef::new("name", "VARCHAR(32)"),
            ],
        });
        match show_index_for_table(&eng, &session, "idx_t").unwrap() {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns[2], "Key_name");
                assert_eq!(columns[3], "Seq_in_index");
                assert_eq!(columns[4], "Column_name");
                assert!(rows.iter().any(|r| r[2] == "PRIMARY" && r[4] == "id"));
                assert!(rows.iter().any(|r| r[2] == "idx_name" && r[4] == "name"));
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn info_schema_schemata_and_statistics() {
        let mut eng = HeapEngine::new();
        eng.create_table(TableMeta {
            name: "idx_t".into(),
            columns: vec![
                ColumnDef {
                    name: "id".into(),
                    data_type: "INT".into(),
                    nullable: false,
                    primary_key: true,
                },
                ColumnDef::new("name", "VARCHAR(32)"),
            ],
        })
        .unwrap();
        eng.create_index(rusql_core::IndexMeta {
            name: "idx_name".into(),
            table: "idx_t".into(),
            column: "name".into(),
        })
        .unwrap();

        match scan_information_schema_schemata(DEFAULT_SCHEMA) {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows[0][0], "rusql");
            }
            _ => panic!("expected rows"),
        }

        let session = rusql_core::Session::new(1, "root");
        let mut session = session;
        session.catalog.create_table(TableMeta {
            name: "idx_t".into(),
            columns: vec![
                ColumnDef {
                    name: "id".into(),
                    data_type: "INT".into(),
                    nullable: false,
                    primary_key: true,
                },
                ColumnDef::new("name", "VARCHAR(32)"),
            ],
        });
        match scan_information_schema_statistics(&eng, &session).unwrap() {
            QueryResult::Rows { rows, .. } => {
                assert!(rows.iter().any(|r| r[2] == "PRIMARY" && r[4] == "id"));
                assert!(rows.iter().any(|r| r[2] == "idx_name" && r[4] == "name"));
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn info_schema_tables() {
        let mut eng = HeapEngine::new();
        eng.create_table(TableMeta {
            name: "a".into(),
            columns: vec![ColumnDef::new("x", "INT")],
        })
        .unwrap();
        let session = rusql_core::Session::new(1, "root");
        match scan_information_schema_tables(&eng, &session, DEFAULT_SCHEMA) {
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

//! Documented `SHOW TABLE STATUS` stubs (M87).
//!
//! Real table **names** from the current schema (same set as `SHOW TABLES`).
//! Other cells are stubs — not InnoDB row-format or tablespace stats.
//! `Rows` MAY be the heap row count; `Auto_increment` MAY match the catalog counter.

use crate::info_schema::DEFAULT_COLLATION;
use crate::{ExecError, QueryResult};
use rusql_core::{table_storage_key, Session, DEFAULT_SCHEMA as CORE_DEFAULT_SCHEMA};
use rusql_storage::{Row, StorageEngine, StorageError};

pub(crate) const TABLE_STATUS_VIRTUAL_TABLE: &str = rusql_sql::TABLE_STATUS_VIRTUAL_TABLE;

pub(crate) const COLUMNS: [&str; 18] = [
    "Name",
    "Engine",
    "Version",
    "Row_format",
    "Rows",
    "Avg_row_length",
    "Data_length",
    "Max_data_length",
    "Index_length",
    "Data_free",
    "Auto_increment",
    "Create_time",
    "Update_time",
    "Check_time",
    "Collation",
    "Checksum",
    "Create_options",
    "Comment",
];

const STUB_ENGINE: &str = "InnoDB";
const STUB_VERSION: &str = "10";
const STUB_ROW_FORMAT: &str = "Dynamic";
const STUB_ZERO: &str = "0";

/// `SHOW TABLE STATUS [{FROM|IN} db] [LIKE …]` over tables in `db` (or the session database).
pub(crate) fn show_table_status<E: StorageEngine>(
    engine: &E,
    session: &Session,
    database: Option<&str>,
    like: Option<&str>,
) -> Result<QueryResult, ExecError> {
    let db = database.unwrap_or(session.database.as_str());
    if !engine.list_databases().iter().any(|d| d == db) {
        return Err(ExecError::Storage(StorageError::database_not_found(db)));
    }
    let mut names = list_show_tables(engine, session, db);
    if let Some(pattern) = like {
        names.retain(|name| like_ci(name, pattern));
    }
    let rows: Vec<Row> = names
        .into_iter()
        .map(|name| table_status_row(engine, session, db, &name))
        .collect();
    Ok(QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows,
    })
}

/// Table names listed by `SHOW TABLES` for `db` (base tables + views).
pub(crate) fn list_show_tables<E: StorageEngine>(
    engine: &E,
    session: &Session,
    db: &str,
) -> Vec<String> {
    let mut tables = engine.table_names_in(db);
    tables.extend(
        session
            .catalog
            .view_names()
            .filter(|v| {
                if db == CORE_DEFAULT_SCHEMA {
                    !v.contains('.')
                } else {
                    v.starts_with(&format!("{db}."))
                }
            })
            .map(|v| {
                v.rsplit_once('.')
                    .map(|(_, n)| n.to_string())
                    .unwrap_or_else(|| v.clone())
            }),
    );
    tables.sort();
    tables.dedup();
    tables
}

fn table_status_row<E: StorageEngine>(engine: &E, session: &Session, db: &str, name: &str) -> Row {
    let key = table_storage_key(db, name);
    let rows = engine.row_count(&key).unwrap_or(0).to_string();
    let auto_increment = session
        .catalog
        .get_table(&key)
        .and_then(|meta| meta.auto_increment_next)
        .map(|n| n.to_string())
        .unwrap_or_default();
    vec![
        name.to_string(),
        STUB_ENGINE.to_string(),
        STUB_VERSION.to_string(),
        STUB_ROW_FORMAT.to_string(),
        rows,
        STUB_ZERO.to_string(),
        STUB_ZERO.to_string(),
        STUB_ZERO.to_string(),
        STUB_ZERO.to_string(),
        STUB_ZERO.to_string(),
        auto_increment,
        String::new(),
        String::new(),
        String::new(),
        DEFAULT_COLLATION.to_string(),
        String::new(),
        String::new(),
        String::new(),
    ]
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
    use rusql_core::{ColumnDef, TableMeta};
    use rusql_storage::HeapEngine;

    fn session_with_table(name: &str, auto_increment: bool) -> (HeapEngine, Session) {
        let mut engine = HeapEngine::new();
        let mut columns = vec![ColumnDef::new("id", "INT")];
        columns[0].primary_key = true;
        columns[0].auto_increment = auto_increment;
        let meta = TableMeta {
            name: name.into(),
            schema: "rusql".into(),
            columns,
            auto_increment_next: if auto_increment { Some(1) } else { None },
            ..Default::default()
        };
        engine.create_table(meta.clone()).unwrap();
        let mut session = Session::new(1, "root");
        session.catalog.create_table(meta);
        (engine, session)
    }

    #[test]
    fn show_table_status_columns_and_like() {
        let (mut engine, session) = session_with_table("alpha", true);
        engine.insert("alpha", vec!["1".into()]).unwrap();

        match show_table_status(&engine, &session, None, None).unwrap() {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "alpha");
                assert_eq!(rows[0][1], STUB_ENGINE);
                assert_eq!(rows[0][2], STUB_VERSION);
                assert_eq!(rows[0][3], STUB_ROW_FORMAT);
                assert_eq!(rows[0][4], "1");
                assert_eq!(rows[0][10], "1");
                assert_eq!(rows[0][14], DEFAULT_COLLATION);
            }
            other => panic!("expected rows, got {other:?}"),
        }

        match show_table_status(&engine, &session, None, Some("a%")).unwrap() {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "alpha");
            }
            other => panic!("expected LIKE rows, got {other:?}"),
        }
        match show_table_status(&engine, &session, None, Some("no_such%")).unwrap() {
            QueryResult::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE, got {other:?}"),
        }

        let err = show_table_status(&engine, &session, Some("missing_db"), None).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("missing_db"),
            "unknown FROM db should mention the name, got {msg}"
        );
    }
}

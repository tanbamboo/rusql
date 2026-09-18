//! Documented `SHOW FUNCTION STATUS` stubs (M98).
//!
//! Catalog `Db` / `Name` from M63 `FunctionMeta`; `Type` is `FUNCTION`.
//! Other cells are stubs — not live DEFINER / timestamps / charset catalogs.

use crate::info_schema::{DEFAULT_CHARSET, DEFAULT_COLLATION};
use crate::QueryResult;
use rusql_core::{FunctionMeta, Session};
use rusql_storage::Row;

pub(crate) const FUNCTION_STATUS_VIRTUAL_TABLE: &str = rusql_sql::FUNCTION_STATUS_VIRTUAL_TABLE;

/// MySQL 8.0 `SHOW FUNCTION STATUS` column order.
pub(crate) const COLUMNS: [&str; 11] = [
    "Db",
    "Name",
    "Type",
    "Definer",
    "Modified",
    "Created",
    "Security_type",
    "Comment",
    "character_set_client",
    "collation_connection",
    "Database Collation",
];

const STUB_DEFINER: &str = "root@%";
const STUB_SECURITY_TYPE: &str = "DEFINER";

/// `SHOW FUNCTION STATUS [LIKE …]` over catalog functions.
pub(crate) fn show_function_status(session: &Session, like: Option<&str>) -> QueryResult {
    let mut functions: Vec<&FunctionMeta> = session.catalog.iter_functions().collect();
    if let Some(pattern) = like {
        functions.retain(|f| like_ci(&f.name, pattern));
    }
    functions.sort_by(|a, b| a.schema.cmp(&b.schema).then_with(|| a.name.cmp(&b.name)));
    let rows: Vec<Row> = functions.into_iter().map(function_row).collect();
    QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows,
    }
}

fn function_row(meta: &FunctionMeta) -> Row {
    vec![
        meta.schema.clone(),
        meta.name.clone(),
        "FUNCTION".to_string(),
        STUB_DEFINER.to_string(),
        String::new(),
        String::new(),
        STUB_SECURITY_TYPE.to_string(),
        String::new(),
        DEFAULT_CHARSET.to_string(),
        DEFAULT_COLLATION.to_string(),
        DEFAULT_COLLATION.to_string(),
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
    use rusql_core::DEFAULT_SCHEMA;

    fn session_with_function(name: &str) -> Session {
        let mut session = Session::new(1, "root");
        session.catalog.create_function(FunctionMeta {
            schema: DEFAULT_SCHEMA.into(),
            name: name.into(),
            return_type: "INT".into(),
            return_expr: "42".into(),
        });
        session
    }

    #[test]
    fn show_function_status_columns_catalog_and_like() {
        let session = session_with_function("f");

        match show_function_status(&session, None) {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], DEFAULT_SCHEMA);
                assert_eq!(rows[0][1], "f");
                assert_eq!(rows[0][2], "FUNCTION");
                assert_eq!(rows[0][3], STUB_DEFINER);
                assert_eq!(rows[0][4], "");
                assert_eq!(rows[0][5], "");
                assert_eq!(rows[0][6], STUB_SECURITY_TYPE);
                assert_eq!(rows[0][7], "");
                assert_eq!(rows[0][8], DEFAULT_CHARSET);
                assert_eq!(rows[0][9], DEFAULT_COLLATION);
                assert_eq!(rows[0][10], DEFAULT_COLLATION);
            }
            other => panic!("expected rows, got {other:?}"),
        }

        match show_function_status(&session, Some("f%")) {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][1], "f");
            }
            other => panic!("expected LIKE rows, got {other:?}"),
        }
        match show_function_status(&session, Some("no_such%")) {
            QueryResult::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE, got {other:?}"),
        }
    }
}

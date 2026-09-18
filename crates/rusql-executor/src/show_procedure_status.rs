//! Documented `SHOW PROCEDURE STATUS` stubs (M97).
//!
//! Catalog `Db` / `Name` from P3 `ProcedureMeta`; `Type` is `PROCEDURE`.
//! Other cells are stubs — not live DEFINER / timestamps / charset catalogs.

use crate::info_schema::{DEFAULT_CHARSET, DEFAULT_COLLATION};
use crate::QueryResult;
use rusql_core::{ProcedureMeta, Session};
use rusql_storage::Row;

pub(crate) const PROCEDURE_STATUS_VIRTUAL_TABLE: &str = rusql_sql::PROCEDURE_STATUS_VIRTUAL_TABLE;

/// MySQL 8.0 `SHOW PROCEDURE STATUS` column order.
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

/// `SHOW PROCEDURE STATUS [LIKE …]` over catalog procedures.
pub(crate) fn show_procedure_status(session: &Session, like: Option<&str>) -> QueryResult {
    let mut procedures: Vec<&ProcedureMeta> = session.catalog.iter_procedures().collect();
    if let Some(pattern) = like {
        procedures.retain(|p| like_ci(&p.name, pattern));
    }
    procedures.sort_by(|a, b| a.schema.cmp(&b.schema).then_with(|| a.name.cmp(&b.name)));
    let rows: Vec<Row> = procedures.into_iter().map(procedure_row).collect();
    QueryResult::Rows {
        columns: COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows,
    }
}

fn procedure_row(meta: &ProcedureMeta) -> Row {
    vec![
        meta.schema.clone(),
        meta.name.clone(),
        "PROCEDURE".to_string(),
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

    fn session_with_procedure(name: &str) -> Session {
        let mut session = Session::new(1, "root");
        session.catalog.create_procedure(ProcedureMeta {
            schema: DEFAULT_SCHEMA.into(),
            name: name.into(),
            body: vec!["INSERT INTO src VALUES (42)".into()],
        });
        session
    }

    #[test]
    fn show_procedure_status_columns_catalog_and_like() {
        let session = session_with_procedure("p");

        match show_procedure_status(&session, None) {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    COLUMNS.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], DEFAULT_SCHEMA);
                assert_eq!(rows[0][1], "p");
                assert_eq!(rows[0][2], "PROCEDURE");
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

        match show_procedure_status(&session, Some("p%")) {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][1], "p");
            }
            other => panic!("expected LIKE rows, got {other:?}"),
        }
        match show_procedure_status(&session, Some("no_such%")) {
            QueryResult::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE, got {other:?}"),
        }
    }
}

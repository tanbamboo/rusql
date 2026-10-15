//! EXPLAIN SELECT plan output (M49).

use rusql_core::table_storage_key;
use rusql_planner::{explain_query_statement, ExplainPlanRow, IndexInfo};
use rusql_storage::StorageEngine;

use crate::{ExecError, QueryResult, Session};

const EXPLAIN_COLUMNS: [&str; 6] = ["id", "select_type", "table", "type", "key", "rows"];
pub const EXPLAIN_JSON_COLUMN: &str = "EXPLAIN";

pub fn explain_statement<E: StorageEngine>(
    engine: &E,
    session: &Session,
    statement: &sqlparser::ast::Statement,
) -> Result<QueryResult, ExecError> {
    let plan = resolve_explain_plan(engine, session, statement)?;
    Ok(plan_to_rows(&plan))
}

/// M145: documented JSON subset for `EXPLAIN FORMAT=JSON` (not full MySQL optimizer tree).
pub fn explain_statement_json<E: StorageEngine>(
    engine: &E,
    session: &Session,
    statement: &sqlparser::ast::Statement,
) -> Result<QueryResult, ExecError> {
    let plan = resolve_explain_plan(engine, session, statement)?;
    Ok(plan_to_json_rows(&plan))
}

fn resolve_explain_plan<E: StorageEngine>(
    engine: &E,
    session: &Session,
    statement: &sqlparser::ast::Statement,
) -> Result<ExplainPlanRow, ExecError> {
    let indexes: Vec<IndexInfo> = engine
        .index_metas()
        .into_iter()
        .map(|m| IndexInfo {
            name: m.name,
            columns: m.columns,
        })
        .collect();
    let table_key = resolve_table_key(session, statement)?;
    let row_count = engine.row_count(&table_key).unwrap_or(1);
    explain_query_statement(statement, &indexes, row_count)
        .ok_or_else(|| ExecError::Message("EXPLAIN supports SELECT queries only".into()))
}

fn resolve_table_key(
    session: &Session,
    statement: &sqlparser::ast::Statement,
) -> Result<String, ExecError> {
    let plan = explain_query_statement(statement, &[], 1)
        .ok_or_else(|| ExecError::Message("EXPLAIN could not resolve table".into()))?;
    Ok(table_storage_key(&session.database, &plan.table))
}

fn plan_to_rows(plan: &ExplainPlanRow) -> QueryResult {
    QueryResult::Rows {
        columns: EXPLAIN_COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows: vec![vec![
            plan.id.to_string(),
            plan.select_type.clone(),
            plan.table.clone(),
            plan.access_type.as_str().into(),
            plan.key.clone().unwrap_or_default(),
            plan.rows.to_string(),
        ]],
    }
}

fn plan_to_json_rows(plan: &ExplainPlanRow) -> QueryResult {
    QueryResult::Rows {
        columns: vec![EXPLAIN_JSON_COLUMN.into()],
        rows: vec![vec![plan_to_json(plan)]],
    }
}

/// Documented stub shape: top-level `query_block` with nested `table.access_type`.
pub fn plan_to_json(plan: &ExplainPlanRow) -> String {
    let mut table = serde_json::Map::new();
    table.insert(
        "table_name".into(),
        serde_json::Value::String(plan.table.clone()),
    );
    table.insert(
        "access_type".into(),
        serde_json::Value::String(plan.access_type.as_str().into()),
    );
    if let Some(key) = &plan.key {
        table.insert("key".into(), serde_json::Value::String(key.clone()));
    }
    table.insert(
        "rows_examined_per_scan".into(),
        serde_json::Value::Number(plan.rows.into()),
    );

    let mut query_block = serde_json::Map::new();
    query_block.insert(
        "select_id".into(),
        serde_json::Value::Number(plan.id.into()),
    );
    query_block.insert(
        "select_type".into(),
        serde_json::Value::String(plan.select_type.clone()),
    );
    query_block.insert("table".into(), serde_json::Value::Object(table));

    let mut root = serde_json::Map::new();
    root.insert("query_block".into(), serde_json::Value::Object(query_block));
    serde_json::to_string(&serde_json::Value::Object(root)).expect("explain json stub")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::{ColumnDef, Session, TableMeta};
    use rusql_sql::parse;
    use rusql_storage::{HeapEngine, StorageEngine};

    #[test]
    fn explain_json_returns_query_block() {
        let mut eng = HeapEngine::new();
        eng.create_table(TableMeta {
            name: "t".into(),
            schema: "rusql".into(),
            columns: vec![ColumnDef::new("id", "INT"), ColumnDef::new("k", "INT")],
            auto_increment_next: None,
            ..Default::default()
        })
        .unwrap();
        eng.create_index(rusql_core::IndexMeta::single_column("idx_k", "t", "k"))
            .unwrap();
        let stmt = parse("EXPLAIN FORMAT=JSON SELECT * FROM t WHERE k = 42").unwrap();
        let session = Session::new(1, "root");
        match explain_statement_json(&eng, &session, &stmt[0]).unwrap() {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, vec!["EXPLAIN".to_string()]);
                let doc: serde_json::Value = serde_json::from_str(&rows[0][0]).unwrap();
                let qb = doc.get("query_block").expect("query_block");
                assert_eq!(qb["select_type"], "SIMPLE");
                assert_eq!(qb["table"]["table_name"], "t");
                assert_eq!(qb["table"]["access_type"], "ref");
                assert_eq!(qb["table"]["key"], "idx_k");
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn explain_json_matches_text_plan() {
        let mut eng = HeapEngine::new();
        eng.create_table(TableMeta {
            name: "t".into(),
            schema: "rusql".into(),
            columns: vec![ColumnDef::new("id", "INT")],
            auto_increment_next: None,
            ..Default::default()
        })
        .unwrap();
        let stmt = parse("EXPLAIN FORMAT=JSON SELECT * FROM t").unwrap();
        let session = Session::new(1, "root");
        let json = explain_statement_json(&eng, &session, &stmt[0]).unwrap();
        let text = explain_statement(&eng, &session, &stmt[0]).unwrap();
        match (json, text) {
            (
                QueryResult::Rows {
                    rows: json_rows, ..
                },
                QueryResult::Rows {
                    rows: text_rows, ..
                },
            ) => {
                let doc: serde_json::Value = serde_json::from_str(&json_rows[0][0]).unwrap();
                assert_eq!(doc["query_block"]["table"]["access_type"], text_rows[0][3]);
                assert_eq!(
                    doc["query_block"]["table"]["table_name"].as_str(),
                    Some(text_rows[0][2].as_str())
                );
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn explain_select_uses_index() {
        let mut eng = HeapEngine::new();
        eng.create_table(TableMeta {
            name: "t".into(),
            schema: "rusql".into(),
            columns: vec![ColumnDef::new("id", "INT"), ColumnDef::new("k", "INT")],
            auto_increment_next: None,
            ..Default::default()
        })
        .unwrap();
        eng.create_index(rusql_core::IndexMeta::single_column("idx_k", "t", "k"))
            .unwrap();
        for i in 0..100 {
            eng.insert("t", vec![i.to_string(), (i * 10).to_string()])
                .unwrap();
        }
        let stmt = parse("EXPLAIN SELECT * FROM t WHERE k = 42").unwrap();
        let session = Session::new(1, "root");
        match explain_statement(&eng, &session, &stmt[0]).unwrap() {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows[0][3], "ref");
                assert_eq!(rows[0][4], "idx_k");
            }
            _ => panic!("expected rows"),
        }
    }
}

//! EXPLAIN SELECT plan output (M49).

use rusql_core::table_storage_key;
use rusql_planner::{explain_query_statement, ExplainPlanRow, IndexInfo};
use rusql_storage::StorageEngine;

use crate::{ExecError, QueryResult, Session};

const EXPLAIN_COLUMNS: [&str; 6] = ["id", "select_type", "table", "type", "key", "rows"];

pub fn explain_statement<E: StorageEngine>(
    engine: &E,
    session: &Session,
    statement: &sqlparser::ast::Statement,
) -> Result<QueryResult, ExecError> {
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
    let plan = explain_query_statement(statement, &indexes, row_count)
        .ok_or_else(|| ExecError::Message("EXPLAIN supports SELECT queries only".into()))?;
    Ok(plan_to_rows(&plan))
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::{ColumnDef, Session, TableMeta};
    use rusql_sql::parse;
    use rusql_storage::{HeapEngine, StorageEngine};

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

//! Query executor for rusql.

use rusql_core::{ColumnDef, Session, TableMeta};
use rusql_planner::Plan;
use rusql_storage::{HeapEngine, Row, StorageEngine};
use sqlparser::ast::{Expr, ObjectName, SelectItem, SetExpr, Statement, TableFactor, Value};
use thiserror::Error;

/// Execution errors.
#[derive(Debug, Error)]
pub enum ExecError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Storage(#[from] rusql_storage::StorageError),
}

/// Query result (MVP).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryResult {
    Ok {
        rows_affected: u64,
    },
    Rows {
        columns: Vec<String>,
        rows: Vec<Row>,
    },
}

/// Execute planned statements against storage.
pub fn execute<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    plans: &[Plan],
) -> Result<Vec<QueryResult>, ExecError> {
    let mut results = Vec::with_capacity(plans.len());
    for plan in plans {
        results.push(execute_one(engine, session, plan)?);
    }
    Ok(results)
}

/// Execute planned statements against an owned engine.
pub struct Executor<E: StorageEngine> {
    engine: E,
}

impl<E: StorageEngine> Executor<E> {
    pub fn new(engine: E) -> Self {
        Self { engine }
    }

    pub fn execute(
        &mut self,
        session: &mut Session,
        plans: &[Plan],
    ) -> Result<Vec<QueryResult>, ExecError> {
        execute(&mut self.engine, session, plans)
    }
}

fn execute_one<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    plan: &Plan,
) -> Result<QueryResult, ExecError> {
    let Plan::Statement(stmt) = plan;
    match stmt {
        Statement::CreateTable(create) => {
            let table_name = object_name_to_string(&create.name);
            let meta = TableMeta {
                name: table_name.clone(),
                columns: create
                    .columns
                    .iter()
                    .map(|c| ColumnDef {
                        name: c.name.value.clone(),
                        data_type: format!("{:?}", c.data_type),
                    })
                    .collect(),
            };
            engine.create_table(meta.clone())?;
            session.catalog.create_table(meta);
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        Statement::Insert(insert) => {
            let table = object_name_to_string(&insert.table_name);
            let rows = extract_insert_values(insert.source.as_deref())?;
            let mut affected = 0u64;
            for row in rows {
                engine.insert(&table, row)?;
                affected += 1;
            }
            Ok(QueryResult::Ok {
                rows_affected: affected,
            })
        }
        Statement::Query(query) => {
            if let SetExpr::Select(select) = query.body.as_ref() {
                if let Some(from) = select.from.first() {
                    if let TableFactor::Table { name, .. } = &from.relation {
                        let table = object_name_to_string(name);
                        let rows = engine.scan(&table)?;
                        let columns = session
                            .catalog
                            .get_table(&table)
                            .map(|m| m.columns.iter().map(|c| c.name.clone()).collect())
                            .unwrap_or_else(|| {
                                if !rows.is_empty() {
                                    (0..rows[0].len())
                                        .map(|i| format!("col{}", i + 1))
                                        .collect()
                                } else {
                                    vec![]
                                }
                            });
                        return Ok(QueryResult::Rows { columns, rows });
                    }
                }
                if select.projection.len() == 1 {
                    if let SelectItem::UnnamedExpr(Expr::Value(Value::Number(n, _))) =
                        &select.projection[0]
                    {
                        return Ok(QueryResult::Rows {
                            columns: vec!["1".into()],
                            rows: vec![vec![n.clone()]],
                        });
                    }
                }
            }
            Ok(QueryResult::Rows {
                columns: vec!["1".into()],
                rows: vec![vec!["1".into()]],
            })
        }
        other => Err(ExecError::Message(format!(
            "unsupported statement: {other:?}"
        ))),
    }
}

fn object_name_to_string(name: &ObjectName) -> String {
    name.0
        .iter()
        .map(|i| i.value.clone())
        .collect::<Vec<_>>()
        .join(".")
}

fn extract_insert_values(source: Option<&sqlparser::ast::Query>) -> Result<Vec<Row>, ExecError> {
    let Some(query) = source else {
        return Ok(vec![]);
    };
    let SetExpr::Values(values) = query.body.as_ref() else {
        return Err(ExecError::Message("INSERT requires VALUES".into()));
    };
    let mut rows = Vec::new();
    for row in &values.rows {
        let mut out = Vec::new();
        for expr in row {
            out.push(expr_to_string(expr)?);
        }
        rows.push(out);
    }
    Ok(rows)
}

fn expr_to_string(expr: &Expr) -> Result<String, ExecError> {
    match expr {
        Expr::Value(Value::Number(n, _)) => Ok(n.clone()),
        Expr::Value(Value::SingleQuotedString(s)) => Ok(s.clone()),
        other => Err(ExecError::Message(format!("unsupported expr: {other:?}"))),
    }
}

/// Convenience constructor with in-memory heap engine.
pub fn heap_executor() -> Executor<HeapEngine> {
    Executor::new(HeapEngine::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_planner::plan;
    use rusql_sql::parse;

    #[test]
    fn create_insert_and_select() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        let create = parse("CREATE TABLE t (id INT)").unwrap();
        let plans = plan(&session, create);
        exec.execute(&mut session, &plans).unwrap();

        let insert = parse("INSERT INTO t VALUES (1)").unwrap();
        let plans = plan(&session, insert);
        let results = exec.execute(&mut session, &plans).unwrap();
        assert_eq!(results[0], QueryResult::Ok { rows_affected: 1 });

        let select = parse("SELECT * FROM t").unwrap();
        let plans = plan(&session, select);
        let results = exec.execute(&mut session, &plans).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["id".to_string()]);
                assert_eq!(rows.len(), 1);
            }
            _ => panic!("expected rows"),
        }
    }
}

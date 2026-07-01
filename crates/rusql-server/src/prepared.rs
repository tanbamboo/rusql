//! Per-connection prepared statement store.

use rusql_core::Session;
use rusql_sql::{bind_placeholders, count_placeholders, parse};
use sqlparser::ast::{SelectItem, SetExpr, Statement, TableFactor};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PreparedStatement {
    pub sql: String,
    pub param_count: usize,
    pub result_columns: Vec<String>,
}

#[derive(Debug, Default)]
pub struct PreparedStatementStore {
    next_id: u32,
    stmts: HashMap<u32, PreparedStatement>,
}

impl PreparedStatementStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            stmts: HashMap::new(),
        }
    }
    pub fn prepare(
        &mut self,
        session: &Session,
        sql: String,
    ) -> Result<(u32, PreparedStatement), String> {
        let param_count = count_placeholders(&sql);
        let check_sql = if param_count > 0 {
            let dummy: Vec<Option<String>> = (0..param_count).map(|_| Some("0".into())).collect();
            bind_placeholders(&sql, &dummy).map_err(|e| e.to_string())?
        } else {
            sql.clone()
        };
        parse(&check_sql).map_err(|e| e.to_string())?;
        let result_columns = infer_result_columns(session, &check_sql)?;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let stmt = PreparedStatement {
            sql,
            param_count,
            result_columns,
        };
        self.stmts.insert(id, stmt.clone());
        Ok((id, stmt))
    }

    pub fn get(&self, id: u32) -> Option<&PreparedStatement> {
        self.stmts.get(&id)
    }

    pub fn close(&mut self, id: u32) {
        self.stmts.remove(&id);
    }

    pub fn bound_sql(&self, id: u32, params: &[Option<String>]) -> Result<String, String> {
        let stmt = self
            .stmts
            .get(&id)
            .ok_or_else(|| "unknown statement id".to_string())?;
        bind_placeholders(&stmt.sql, params).map_err(|e| e.to_string())
    }
}

fn infer_result_columns(session: &Session, sql: &str) -> Result<Vec<String>, String> {
    let stmts = parse(sql).map_err(|e| e.to_string())?;
    let Some(stmt) = stmts.first() else {
        return Ok(vec![]);
    };
    match stmt {
        Statement::ShowTables { .. } => Ok(vec!["Tables_in_rusql".into()]),
        Statement::ShowDatabases { .. } => Ok(vec!["Database".into()]),
        Statement::Query(query) => {
            if let SetExpr::Select(select) = query.body.as_ref() {
                if let Some(from) = select.from.first() {
                    if let TableFactor::Table { name, .. } = &from.relation {
                        let table = name
                            .0
                            .iter()
                            .map(|i| i.value.clone())
                            .collect::<Vec<_>>()
                            .join(".");
                        if let Some(meta) = session.catalog.get_table(&table) {
                            return Ok(meta.columns.iter().map(|c| c.name.clone()).collect());
                        }
                    }
                }
                if select.projection.len() == 1 {
                    if let SelectItem::UnnamedExpr(sqlparser::ast::Expr::Value(
                        sqlparser::ast::Value::Number(n, _),
                    )) = &select.projection[0]
                    {
                        return Ok(vec![n.clone()]);
                    }
                }
            }
            Ok(vec!["1".into()])
        }
        _ => Ok(vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::Session;

    #[test]
    fn prepare_select_literal() {
        let mut store = PreparedStatementStore::new();
        let session = Session::new(1, "root");
        let (id, stmt) = store.prepare(&session, "SELECT 1".into()).unwrap();
        assert_eq!(id, 1);
        assert_eq!(stmt.param_count, 0);
        assert_eq!(stmt.result_columns, vec!["1".to_string()]);
    }
}

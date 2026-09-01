//! Per-connection prepared statement store.

use rusql_core::Session;
use rusql_protocol::{mysql_type_for_result_column, mysql_type_from_sql_type};
use rusql_sql::{bind_placeholders, count_placeholders, parse};
use sqlparser::ast::{SelectItem, SetExpr, Statement, TableFactor};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PreparedStatement {
    pub sql: String,
    pub param_count: usize,
    pub result_columns: Vec<String>,
    pub result_column_types: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct PreparedStatementStore {
    next_id: u32,
    stmts: HashMap<u32, PreparedStatement>,
    long_data: HashMap<u32, HashMap<u16, Vec<u8>>>,
}

impl PreparedStatementStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            stmts: HashMap::new(),
            long_data: HashMap::new(),
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
        let (result_columns, result_column_types) = infer_result_columns(session, &check_sql)?;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let stmt = PreparedStatement {
            sql,
            param_count,
            result_columns,
            result_column_types,
        };
        self.stmts.insert(id, stmt.clone());
        Ok((id, stmt))
    }

    pub fn get(&self, id: u32) -> Option<&PreparedStatement> {
        self.stmts.get(&id)
    }

    pub fn close(&mut self, id: u32) {
        self.stmts.remove(&id);
        self.long_data.remove(&id);
    }

    pub fn reset(&mut self, id: u32) -> bool {
        if self.stmts.contains_key(&id) {
            self.long_data.remove(&id);
            true
        } else {
            false
        }
    }

    pub fn append_long_data(
        &mut self,
        stmt_id: u32,
        param_id: u16,
        data: &[u8],
    ) -> Result<(), String> {
        if !self.stmts.contains_key(&stmt_id) {
            return Err("unknown prepared statement handler".into());
        }
        self.long_data
            .entry(stmt_id)
            .or_default()
            .entry(param_id)
            .or_default()
            .extend_from_slice(data);
        Ok(())
    }

    pub fn bound_sql(&self, id: u32, params: &[Option<String>]) -> Result<String, String> {
        let stmt = self
            .stmts
            .get(&id)
            .ok_or_else(|| "unknown statement id".to_string())?;
        let merged = self.merge_long_params(id, params);
        bind_placeholders(&stmt.sql, &merged).map_err(|e| e.to_string())
    }

    pub fn take_long_data(&mut self, id: u32) {
        self.long_data.remove(&id);
    }

    fn merge_long_params(&self, id: u32, params: &[Option<String>]) -> Vec<Option<String>> {
        let mut merged = params.to_vec();
        if let Some(long_map) = self.long_data.get(&id) {
            for (param_id, data) in long_map {
                let idx = *param_id as usize;
                if idx < merged.len() {
                    merged[idx] = Some(String::from_utf8_lossy(data).into_owned());
                }
            }
        }
        merged
    }
}

fn infer_result_columns(session: &Session, sql: &str) -> Result<(Vec<String>, Vec<u8>), String> {
    let stmts = parse(sql).map_err(|e| e.to_string())?;
    let Some(stmt) = stmts.first() else {
        return Ok((vec![], vec![]));
    };
    match stmt {
        Statement::ShowTables { .. } => Ok((
            vec!["Tables_in_rusql".into()],
            vec![mysql_type_from_sql_type("VARCHAR")],
        )),
        Statement::ShowDatabases { .. } => Ok((
            vec!["Database".into()],
            vec![mysql_type_from_sql_type("VARCHAR")],
        )),
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
                        if table == "__rusql_show_index" {
                            return Ok((
                                vec![
                                    "Table".into(),
                                    "Non_unique".into(),
                                    "Key_name".into(),
                                    "Seq_in_index".into(),
                                    "Column_name".into(),
                                    "Index_type".into(),
                                ],
                                vec![mysql_type_from_sql_type("VARCHAR"); 6],
                            ));
                        }
                        if let Some(meta) = session.catalog.get_table(&table) {
                            let columns: Vec<String> =
                                meta.columns.iter().map(|c| c.name.clone()).collect();
                            let types = meta
                                .columns
                                .iter()
                                .map(|c| mysql_type_from_sql_type(&c.data_type))
                                .collect();
                            return Ok((columns, types));
                        }
                    }
                }
                if select.projection.len() == 1 {
                    if let SelectItem::UnnamedExpr(sqlparser::ast::Expr::Value(
                        sqlparser::ast::Value::Number(n, _),
                    )) = &select.projection[0]
                    {
                        return Ok((vec![n.clone()], vec![mysql_type_for_result_column(n)]));
                    }
                }
            }
            Ok((vec!["1".into()], vec![mysql_type_for_result_column("1")]))
        }
        _ => Ok((vec![], vec![])),
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
        assert_eq!(
            stmt.result_column_types,
            vec![rusql_protocol::MYSQL_TYPE_LONGLONG]
        );
    }

    #[test]
    fn long_data_merged_at_bind() {
        let mut store = PreparedStatementStore::new();
        let session = Session::new(1, "root");
        let (id, _) = store.prepare(&session, "SELECT ?".into()).unwrap();
        store.append_long_data(id, 0, b"hello").unwrap();
        store.append_long_data(id, 0, b" world").unwrap();
        let sql = store.bound_sql(id, &[None]).unwrap();
        assert_eq!(sql, "SELECT 'hello world'");
    }
}

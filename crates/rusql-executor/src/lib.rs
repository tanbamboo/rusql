//! Query executor for rusql.

mod info_schema;

use rusql_core::{ColumnDef, IndexMeta, Session, TableMeta};
use rusql_planner::Plan;
use rusql_storage::{ColumnAssignment, DeleteFilter, HeapEngine, Row, StorageEngine};
use sqlparser::ast::{
    Assignment, AssignmentTarget, BinaryOperator, DescribeAlias, Expr, FromTable, ObjectName,
    ObjectType, SelectItem, SetExpr, ShowCreateObject, Statement, TableFactor, Use, Value,
};
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
                        data_type: c.data_type.to_string(),
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
        Statement::CreateIndex(create) => {
            let table = object_name_to_string(&create.table_name);
            let order_col = create
                .columns
                .first()
                .ok_or_else(|| ExecError::Message("CREATE INDEX requires a column".into()))?;
            let column = match &order_col.expr {
                Expr::Identifier(id) => id.value.clone(),
                other => {
                    return Err(ExecError::Message(format!(
                        "unsupported index column expr: {other:?}"
                    )))
                }
            };
            let name = create
                .name
                .as_ref()
                .map(object_name_to_string)
                .unwrap_or_else(|| format!("idx_{table}_{column}"));
            let meta = IndexMeta {
                name,
                table,
                column,
            };
            engine.create_index(meta)?;
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        Statement::Drop {
            object_type,
            names,
            if_exists,
            ..
        } => {
            if *object_type != ObjectType::Table {
                return Err(ExecError::Message(format!(
                    "unsupported DROP type: {object_type}"
                )));
            }
            let mut affected = 0u64;
            for name in names {
                let table = object_name_to_string(name);
                match engine.drop_table(&table) {
                    Ok(()) => {
                        session.catalog.drop_table(&table);
                        affected += 1;
                    }
                    Err(_) if *if_exists => continue,
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(QueryResult::Ok {
                rows_affected: affected,
            })
        }
        Statement::Delete(delete) => {
            let table = delete_table_name(delete)?;
            let filter = extract_eq_predicate(delete.selection.as_ref())
                .map(|(column, value)| DeleteFilter { column, value });
            let affected = engine.delete_rows(&table, filter)?;
            Ok(QueryResult::Ok {
                rows_affected: affected,
            })
        }
        Statement::Update {
            table,
            assignments,
            selection,
            ..
        } => {
            let table_name = table_name_from_table_with_joins(table)?;
            let assigns = extract_assignments(assignments)?;
            let filter = extract_eq_predicate(selection.as_ref())
                .map(|(column, value)| DeleteFilter { column, value });
            let affected = engine.update_rows(&table_name, &assigns, filter)?;
            Ok(QueryResult::Ok {
                rows_affected: affected,
            })
        }
        Statement::ExplainTable {
            describe_alias,
            table_name,
            ..
        } => {
            if !matches!(
                describe_alias,
                DescribeAlias::Describe | DescribeAlias::Desc
            ) {
                return Err(ExecError::Message(
                    "EXPLAIN is not supported yet; use DESCRIBE tbl".into(),
                ));
            }
            let table = object_name_to_string(table_name);
            info_schema::describe_table_by_name(session, &table)
        }
        Statement::ShowColumns { show_options, .. } => {
            let table = show_options
                .show_in
                .as_ref()
                .and_then(|i| i.parent_name.as_ref())
                .map(object_name_to_string)
                .ok_or_else(|| ExecError::Message("SHOW COLUMNS requires a table".into()))?;
            info_schema::describe_table_by_name(session, &table)
        }
        Statement::ShowCreate { obj_type, obj_name } => {
            if *obj_type != ShowCreateObject::Table {
                return Err(ExecError::Message(format!(
                    "unsupported SHOW CREATE type: {obj_type}"
                )));
            }
            let table = object_name_to_string(obj_name);
            info_schema::show_create_table_by_name(session, &table)
        }
        Statement::Use(use_expr) => {
            let db = use_database_name(use_expr)?;
            if db != info_schema::DEFAULT_SCHEMA {
                return Err(ExecError::Message(format!("unknown database '{db}'")));
            }
            session.database = db;
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        Statement::ShowTables { .. } => {
            let db = "rusql".to_string();
            let col = format!("Tables_in_{db}");
            let mut tables = engine.table_names();
            tables.sort();
            let rows: Vec<Row> = tables.into_iter().map(|t| vec![t]).collect();
            Ok(QueryResult::Rows {
                columns: vec![col],
                rows,
            })
        }
        Statement::ShowDatabases { .. } => Ok(QueryResult::Rows {
            columns: vec!["Database".into()],
            rows: vec![vec!["rusql".into()]],
        }),
        Statement::Query(query) => {
            if let SetExpr::Select(select) = query.body.as_ref() {
                if let Some(from) = select.from.first() {
                    if let TableFactor::Table { name, .. } = &from.relation {
                        let table = object_name_to_string(name);
                        if let Some(kind) = info_schema::is_information_schema_table(&table) {
                            let table_filter = if kind == "columns" {
                                extract_eq_predicate(select.selection.as_ref())
                                    .filter(|(col, _)| col.eq_ignore_ascii_case("table_name"))
                                    .map(|(_, v)| v)
                            } else {
                                None
                            };
                            let result = match kind {
                                "tables" => info_schema::scan_information_schema_tables(
                                    engine,
                                    &session.database,
                                ),
                                "columns" => info_schema::scan_information_schema_columns(
                                    engine,
                                    session,
                                    table_filter.as_deref(),
                                )?,
                                _ => unreachable!(),
                            };
                            return Ok(result);
                        }
                        let table_columns: Vec<String> = session
                            .catalog
                            .get_table(&table)
                            .map(|m| m.columns.iter().map(|c| c.name.clone()).collect())
                            .unwrap_or_default();
                        let (out_columns, proj_indices) =
                            resolve_projection(&select.projection, &table_columns)?;
                        let rows = if let Some((col, val)) =
                            extract_eq_predicate(select.selection.as_ref())
                        {
                            match engine.scan_eq(&table, &col, &val)? {
                                Some(indexed) => indexed,
                                None => filter_rows_by_eq(
                                    engine.scan(&table)?,
                                    &table_columns,
                                    &col,
                                    &val,
                                )?,
                            }
                        } else {
                            engine.scan(&table)?
                        };
                        let (columns, rows) =
                            finalize_select_rows(out_columns, proj_indices, table_columns, rows)?;
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

fn use_database_name(use_expr: &Use) -> Result<String, ExecError> {
    let name = match use_expr {
        Use::Object(n) | Use::Database(n) | Use::Schema(n) => object_name_to_string(n),
        Use::Default => return Ok(info_schema::DEFAULT_SCHEMA.into()),
        other => {
            return Err(ExecError::Message(format!(
                "unsupported USE statement: {other}"
            )))
        }
    };
    Ok(name.split('.').next().unwrap_or(name.as_str()).to_string())
}

fn delete_table_name(delete: &sqlparser::ast::Delete) -> Result<String, ExecError> {
    let tables = match &delete.from {
        FromTable::WithFromKeyword(t) | FromTable::WithoutKeyword(t) => t,
    };
    let first = tables
        .first()
        .ok_or_else(|| ExecError::Message("DELETE requires a table".into()))?;
    table_name_from_table_factor(&first.relation)
}

fn table_name_from_table_with_joins(
    table: &sqlparser::ast::TableWithJoins,
) -> Result<String, ExecError> {
    table_name_from_table_factor(&table.relation)
}

fn table_name_from_table_factor(relation: &TableFactor) -> Result<String, ExecError> {
    match relation {
        TableFactor::Table { name, .. } => Ok(object_name_to_string(name)),
        other => Err(ExecError::Message(format!("unsupported table: {other:?}"))),
    }
}

fn extract_assignments(assignments: &[Assignment]) -> Result<Vec<ColumnAssignment>, ExecError> {
    let mut out = Vec::with_capacity(assignments.len());
    for a in assignments {
        let column = match &a.target {
            AssignmentTarget::ColumnName(name) => object_name_to_string(name),
            other => {
                return Err(ExecError::Message(format!(
                    "unsupported assignment target: {other:?}"
                )))
            }
        };
        out.push(ColumnAssignment {
            column,
            value: expr_to_string(&a.value)?,
        });
    }
    Ok(out)
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

fn extract_eq_predicate(selection: Option<&Expr>) -> Option<(String, String)> {
    let Expr::BinaryOp {
        left,
        op: BinaryOperator::Eq,
        right,
    } = selection?
    else {
        return None;
    };
    let column = match left.as_ref() {
        Expr::Identifier(id) => id.value.clone(),
        Expr::CompoundIdentifier(parts) => parts.last()?.value.clone(),
        _ => return None,
    };
    let value = expr_to_string(right).ok()?;
    Some((column, value))
}

fn filter_rows_by_eq(
    rows: Vec<Row>,
    columns: &[String],
    column: &str,
    value: &str,
) -> Result<Vec<Row>, ExecError> {
    let col_idx = columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case(column))
        .ok_or_else(|| ExecError::Message(format!("column '{column}' not found")))?;
    Ok(rows
        .into_iter()
        .filter(|r| r.get(col_idx).map(|v| v == value).unwrap_or(false))
        .collect())
}

/// `SELECT` list → output column names and optional source indices (`None` = `*`).
fn resolve_projection(
    projection: &[SelectItem],
    table_columns: &[String],
) -> Result<(Vec<String>, Option<Vec<usize>>), ExecError> {
    if projection.len() == 1 {
        match &projection[0] {
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => {
                return Ok((table_columns.to_vec(), None));
            }
            _ => {}
        }
    }
    let mut names = Vec::with_capacity(projection.len());
    let mut indices = Vec::with_capacity(projection.len());
    for item in projection {
        let (expr, alias) = match item {
            SelectItem::UnnamedExpr(expr) => (expr, None),
            SelectItem::ExprWithAlias { expr, alias } => (expr, Some(alias.value.clone())),
            other => {
                return Err(ExecError::Message(format!(
                    "unsupported SELECT item: {other:?}"
                )))
            }
        };
        let col = expr_column_name(expr)?;
        let idx = table_columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case(&col))
            .ok_or_else(|| ExecError::Message(format!("unknown column '{col}'")))?;
        names.push(alias.unwrap_or(col));
        indices.push(idx);
    }
    Ok((names, Some(indices)))
}

fn expr_column_name(expr: &Expr) -> Result<String, ExecError> {
    match expr {
        Expr::Identifier(id) => Ok(id.value.clone()),
        Expr::CompoundIdentifier(parts) => parts
            .last()
            .map(|id| id.value.clone())
            .ok_or_else(|| ExecError::Message("empty compound identifier".into())),
        other => Err(ExecError::Message(format!(
            "unsupported SELECT expression: {other:?}"
        ))),
    }
}

fn project_rows(rows: Vec<Row>, indices: &[usize]) -> Vec<Row> {
    rows.into_iter()
        .map(|row| indices.iter().map(|i| row[*i].clone()).collect())
        .collect()
}

fn finalize_select_rows(
    out_columns: Vec<String>,
    proj_indices: Option<Vec<usize>>,
    table_columns: Vec<String>,
    rows: Vec<Row>,
) -> Result<(Vec<String>, Vec<Row>), ExecError> {
    let columns = if out_columns.is_empty() && table_columns.is_empty() {
        if rows.is_empty() {
            vec![]
        } else {
            (0..rows[0].len())
                .map(|i| format!("col{}", i + 1))
                .collect()
        }
    } else if out_columns.is_empty() {
        table_columns
    } else {
        out_columns
    };
    let rows = match proj_indices {
        Some(indices) => project_rows(rows, &indices),
        None => rows,
    };
    Ok((columns, rows))
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

    #[test]
    fn create_index_and_where_lookup() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE t (id INT, name VARCHAR(32))",
            "INSERT INTO t VALUES (1, 'a')",
            "INSERT INTO t VALUES (2, 'b')",
            "CREATE INDEX idx_id ON t (id)",
        ] {
            let stmts = parse(sql).unwrap();
            let plans = plan(&session, stmts);
            exec.execute(&mut session, &plans).unwrap();
        }

        let select = parse("SELECT * FROM t WHERE id = 2").unwrap();
        let plans = plan(&session, select);
        let results = exec.execute(&mut session, &plans).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows, &vec![vec!["2".to_string(), "b".to_string()]]);
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn show_tables_lists_created_table() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        let create = parse("CREATE TABLE items (id INT)").unwrap();
        let plans = plan(&session, create);
        exec.execute(&mut session, &plans).unwrap();

        let show = parse("SHOW TABLES").unwrap();
        let plans = plan(&session, show);
        let results = exec.execute(&mut session, &plans).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["Tables_in_rusql".to_string()]);
                assert_eq!(rows, &vec![vec!["items".to_string()]]);
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn show_create_table_statement() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        let create = parse("CREATE TABLE items (id INT, label VARCHAR(16))").unwrap();
        let plans = plan(&session, create);
        exec.execute(&mut session, &plans).unwrap();

        let show = parse("SHOW CREATE TABLE items").unwrap();
        let plans = plan(&session, show);
        let results = exec.execute(&mut session, &plans).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns[0], "Table");
                assert_eq!(rows[0][0], "items");
                assert!(rows[0][1].contains("CREATE TABLE `items`"));
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn describe_and_show_columns() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        let create = parse("CREATE TABLE users (id INT, name VARCHAR(32))").unwrap();
        let plans = plan(&session, create);
        exec.execute(&mut session, &plans).unwrap();

        for sql in ["DESCRIBE users", "SHOW COLUMNS FROM users"] {
            let stmts = parse(sql).unwrap();
            let plans = plan(&session, stmts);
            let results = exec.execute(&mut session, &plans).unwrap();
            match &results[0] {
                QueryResult::Rows { columns, rows } => {
                    assert_eq!(columns[0], "Field");
                    assert_eq!(rows.len(), 2);
                    assert_eq!(rows[0][0], "id");
                    assert_eq!(rows[1][0], "name");
                }
                _ => panic!("expected rows for {sql}"),
            }
        }
    }

    #[test]
    fn use_database() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();

        let plans = plan(&session, parse("USE rusql").unwrap());
        let results = exec.execute(&mut session, &plans).unwrap();
        assert_eq!(results[0], QueryResult::Ok { rows_affected: 0 });
        assert_eq!(session.database, "rusql");

        let plans = plan(&session, parse("USE unknown_db").unwrap());
        assert!(exec.execute(&mut session, &plans).is_err());
    }

    #[test]
    fn select_column_projection() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE users (id INT, name VARCHAR(32))",
            "INSERT INTO users VALUES (1, 'alice')",
            "INSERT INTO users VALUES (2, 'bob')",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans).unwrap();
        }

        let plans = plan(&session, parse("SELECT name FROM users").unwrap());
        let results = exec.execute(&mut session, &plans).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["name".to_string()]);
                assert_eq!(
                    rows,
                    &vec![vec!["alice".to_string()], vec!["bob".to_string()]]
                );
            }
            _ => panic!("expected rows"),
        }

        let plans = plan(
            &session,
            parse("SELECT id, name FROM users WHERE id = 2").unwrap(),
        );
        let results = exec.execute(&mut session, &plans).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["id".to_string(), "name".to_string()]);
                assert_eq!(rows, &vec![vec!["2".to_string(), "bob".to_string()]]);
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn information_schema_tables_and_columns() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        let create = parse("CREATE TABLE t (id INT)").unwrap();
        let plans = plan(&session, create);
        exec.execute(&mut session, &plans).unwrap();

        let tables = parse("SELECT * FROM information_schema.tables").unwrap();
        let plans = plan(&session, tables);
        let results = exec.execute(&mut session, &plans).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns[0], "TABLE_SCHEMA");
                assert_eq!(
                    rows,
                    &vec![vec![
                        "rusql".to_string(),
                        "t".to_string(),
                        "BASE TABLE".to_string(),
                    ]]
                );
            }
            _ => panic!("expected rows"),
        }

        let cols =
            parse("SELECT * FROM information_schema.columns WHERE table_name = 't'").unwrap();
        let plans = plan(&session, cols);
        let results = exec.execute(&mut session, &plans).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][2], "id");
            }
            _ => panic!("expected rows"),
        }
    }
}

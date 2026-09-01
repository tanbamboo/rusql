//! Query executor for rusql.

mod aggregate;
mod explain;
mod expr;
mod fk;
mod info_schema;
mod privileges;
mod subquery;
mod where_filter;

pub use info_schema::DEFAULT_SCHEMA;
pub use privileges::{
    check_statement_privilege, execute_grant, execute_revoke, mysql_user_stub_rows,
    show_grants_result, MYSQL_USER_VIRTUAL_TABLE, SHOW_GRANTS_VIRTUAL_TABLE,
};

use crate::aggregate::{execute_group_by, select_has_group_by};
use crate::expr::{eval_expr, expr_output_name};
use crate::fk::{
    apply_assignments, check_delete, check_insert, check_update, foreign_key_from_constraint,
    matching_rows, validate_foreign_keys,
};
use crate::subquery::{
    eval_scalar_subquery, filter_inline_rows, parse_where_with_subqueries, select_from_subquery,
};

use crate::where_filter::{
    between_predicate_from_filter, eq_predicate_from_filter, eq_prefix_from_filter,
    extract_eq_predicate,
};
use rusql_core::{
    normalize_column_type, table_storage_key, ColumnDef, IndexMeta, PrivilegeStore, Session,
    TableMeta, ViewMeta, DEFAULT_SCHEMA as CORE_DEFAULT_SCHEMA,
};
use rusql_planner::Plan;
use rusql_storage::{ColumnAssignment, DeleteFilter, HeapEngine, Row, StorageEngine, StorageError};
use sqlparser::ast::{
    AlterTableOperation, Assignment, AssignmentTarget, BinaryOperator, ColumnOption, DescribeAlias,
    Expr, FromTable, JoinConstraint, JoinOperator, ObjectName, ObjectType, Offset, OrderBy,
    SelectItem, SetExpr, SetOperator, SetQuantifier, ShowCreateObject, Statement, TableConstraint,
    TableFactor, Use, Value,
};
use std::collections::HashSet;
use thiserror::Error;

/// Execution errors.
#[derive(Debug, Error)]
pub enum ExecError {
    #[error("{0}")]
    Message(String),
    #[error("{message}")]
    Mysql { code: u16, message: String },
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
    privileges: Option<&PrivilegeStore>,
) -> Result<Vec<QueryResult>, ExecError> {
    let default_store = PrivilegeStore::new();
    let store = privileges.unwrap_or(&default_store);
    let mut results = Vec::with_capacity(plans.len());
    for plan in plans {
        results.push(execute_one(engine, session, plan, store)?);
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
        privileges: Option<&PrivilegeStore>,
    ) -> Result<Vec<QueryResult>, ExecError> {
        execute(&mut self.engine, session, plans, privileges)
    }
}

fn execute_one<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    plan: &Plan,
    privileges: &PrivilegeStore,
) -> Result<QueryResult, ExecError> {
    let Plan::Statement(stmt) = plan;
    match stmt {
        Statement::CreateTable(create) => {
            let meta = table_meta_from_create(create, &session.database)?;
            validate_foreign_keys(session, &meta)?;
            engine.create_table(meta.clone())?;
            session.catalog.create_table(meta);
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        Statement::CreateDatabase {
            db_name,
            if_not_exists,
            ..
        } => {
            let name = object_name_to_string(db_name);
            match engine.create_database(&name) {
                Ok(()) => Ok(QueryResult::Ok { rows_affected: 0 }),
                Err(_) if *if_not_exists => Ok(QueryResult::Ok { rows_affected: 0 }),
                Err(e) => Err(e.into()),
            }
        }
        Statement::CreateView {
            materialized,
            name,
            query,
            if_not_exists,
            or_replace,
            ..
        } => {
            if *materialized {
                return Err(ExecError::Message(
                    "materialized views are not supported".into(),
                ));
            }
            if *or_replace {
                return Err(ExecError::Message(
                    "OR REPLACE VIEW is not supported".into(),
                ));
            }
            let view_name = resolve_object_storage_key(session, name)?;
            if session.catalog.get_table(&view_name).is_some() {
                return Err(ExecError::Message(format!(
                    "table '{view_name}' already exists"
                )));
            }
            if session.catalog.get_view(&view_name).is_some() {
                if *if_not_exists {
                    return Ok(QueryResult::Ok { rows_affected: 0 });
                }
                return Err(ExecError::Message(format!(
                    "view '{view_name}' already exists"
                )));
            }
            let sql = query.to_string();
            rusql_sql::parse(&sql).map_err(|e| ExecError::Message(e.to_string()))?;
            session.catalog.create_view(ViewMeta {
                name: view_name,
                sql,
            });
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        Statement::Insert(insert) => {
            let table = resolve_object_storage_key(session, &insert.table_name)?;
            let meta = session
                .catalog
                .get_table(&table)
                .cloned()
                .ok_or_else(|| ExecError::Storage(StorageError::table_not_found(&table)))?;
            let value_rows = extract_insert_values(insert.source.as_deref())?;
            let mut affected = 0u64;
            let mut next_ai = meta.auto_increment_next;
            for values in value_rows {
                let (row, bumped) = expand_insert_row(&meta, &insert.columns, values, next_ai)?;
                if let Some(n) = bumped {
                    next_ai = Some(n);
                }
                check_insert(engine, session, &meta, &row)?;
                engine.insert(&table, row)?;
                affected += 1;
            }
            if next_ai != meta.auto_increment_next {
                if let Some(n) = next_ai {
                    engine.set_auto_increment(&table, n)?;
                    let mut updated = meta;
                    updated.auto_increment_next = Some(n);
                    session.catalog.create_table(updated);
                }
            }
            Ok(QueryResult::Ok {
                rows_affected: affected,
            })
        }
        Statement::CreateIndex(create) => {
            let table = resolve_object_storage_key(session, &create.table_name)?;
            let mut columns = Vec::new();
            for order_col in &create.columns {
                let column = match &order_col.expr {
                    Expr::Identifier(id) => id.value.clone(),
                    other => {
                        return Err(ExecError::Message(format!(
                            "unsupported index column expr: {other:?}"
                        )))
                    }
                };
                columns.push(column);
            }
            if columns.is_empty() {
                return Err(ExecError::Message("CREATE INDEX requires a column".into()));
            }
            let lead = columns[0].clone();
            let name = create
                .name
                .as_ref()
                .map(object_name_to_string)
                .unwrap_or_else(|| format!("idx_{table}_{lead}"));
            let meta = IndexMeta {
                name,
                table,
                columns,
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
            if *object_type == ObjectType::Database || *object_type == ObjectType::Schema {
                let mut affected = 0u64;
                for name in names {
                    let db = object_name_to_string(name);
                    match engine.drop_database(&db) {
                        Ok(()) => {
                            if session.database == db {
                                session.database = CORE_DEFAULT_SCHEMA.into();
                            }
                            affected += 1;
                        }
                        Err(_) if *if_exists => continue,
                        Err(e) => return Err(e.into()),
                    }
                }
                return Ok(QueryResult::Ok {
                    rows_affected: affected,
                });
            }
            if *object_type != ObjectType::Table {
                return Err(ExecError::Message(format!(
                    "unsupported DROP type: {object_type}"
                )));
            }
            let mut affected = 0u64;
            for name in names {
                let table = resolve_object_storage_key(session, name)?;
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
            let table = resolve_object_storage_key(session, &delete_table_object_name(delete)?)?;
            let meta = session
                .catalog
                .get_table(&table)
                .cloned()
                .ok_or_else(|| ExecError::Storage(StorageError::table_not_found(&table)))?;
            let filter = extract_eq_predicate(delete.selection.as_ref())
                .map(|(column, value)| DeleteFilter { column, value });
            let rows = matching_rows(engine, &meta, filter.as_ref())?;
            check_delete(engine, session, &meta, &rows)?;
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
            let table_name = resolve_table_with_joins(session, table)?;
            let meta = session
                .catalog
                .get_table(&table_name)
                .cloned()
                .ok_or_else(|| ExecError::Storage(StorageError::table_not_found(&table_name)))?;
            let assigns = extract_assignments(assignments)?;
            let filter = extract_eq_predicate(selection.as_ref())
                .map(|(column, value)| DeleteFilter { column, value });
            let rows = matching_rows(engine, &meta, filter.as_ref())?;
            for row in &rows {
                let new_row = apply_assignments(&meta, row, &assigns)?;
                check_update(engine, session, &meta, row, &new_row)?;
            }
            let affected = engine.update_rows(&table_name, &assigns, filter)?;
            Ok(QueryResult::Ok {
                rows_affected: affected,
            })
        }
        Statement::Explain {
            analyze,
            verbose,
            query_plan,
            statement,
            ..
        } => {
            if *analyze || *verbose || *query_plan {
                return Err(ExecError::Message(
                    "EXPLAIN ANALYZE/VERBOSE/QUERY PLAN not supported".into(),
                ));
            }
            explain::explain_statement(engine, session, statement.as_ref())
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
            let table = resolve_object_storage_key(session, table_name)?;
            info_schema::describe_table_by_name(session, &table)
        }
        Statement::ShowColumns { show_options, .. } => {
            let table = show_options
                .show_in
                .as_ref()
                .and_then(|i| i.parent_name.as_ref())
                .ok_or_else(|| ExecError::Message("SHOW COLUMNS requires a table".into()))?;
            let table = resolve_object_storage_key(session, table)?;
            info_schema::describe_table_by_name(session, &table)
        }
        Statement::ShowCreate { obj_type, obj_name } => {
            if *obj_type != ShowCreateObject::Table {
                return Err(ExecError::Message(format!(
                    "unsupported SHOW CREATE type: {obj_type}"
                )));
            }
            let table = resolve_object_storage_key(session, obj_name)?;
            info_schema::show_create_table_by_name(session, &table)
        }
        Statement::AlterTable {
            name, operations, ..
        } => execute_alter_table(engine, session, name, operations),
        Statement::Use(use_expr) => {
            let db = use_database_name(use_expr)?;
            if !engine.list_databases().iter().any(|d| d == &db) {
                return Err(ExecError::Storage(StorageError::database_not_found(&db)));
            }
            session.database = db;
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        Statement::ShowTables { .. } => {
            let db = session.database.clone();
            let col = format!("Tables_in_{db}");
            let mut tables = engine.table_names_in(&db);
            tables.extend(
                session
                    .catalog
                    .view_names()
                    .filter(|v| {
                        // views stored under storage key for non-default schema
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
            let rows: Vec<Row> = tables.into_iter().map(|t| vec![t]).collect();
            Ok(QueryResult::Rows {
                columns: vec![col],
                rows,
            })
        }
        Statement::ShowDatabases { .. } => {
            let rows: Vec<Row> = engine
                .list_databases()
                .into_iter()
                .map(|d| vec![d])
                .collect();
            Ok(QueryResult::Rows {
                columns: vec!["Database".into()],
                rows,
            })
        }
        Statement::Query(query) => {
            let limit = extract_limit(query.limit.as_ref())?;
            let offset = extract_offset(query.offset.as_ref())?;
            let order_by = query.order_by.as_ref();
            if matches!(query.body.as_ref(), SetExpr::SetOperation { .. }) {
                let (columns, rows) =
                    execute_set_expr(engine, session, query.body.as_ref(), privileges)?;
                let rows = finish_row_set(rows, &columns, order_by, offset, limit)?;
                return Ok(QueryResult::Rows { columns, rows });
            }
            if let SetExpr::Select(select) = query.body.as_ref() {
                if let Some(from) = select.from.first() {
                    if !from.joins.is_empty() {
                        return execute_join_select(
                            engine, session, select, from, order_by, offset, limit,
                        );
                    }
                    if let TableFactor::Derived { subquery, .. } = &from.relation {
                        return execute_derived_select(
                            engine, session, select, subquery, order_by, offset, limit,
                        );
                    }
                    if let TableFactor::Table { name, .. } = &from.relation {
                        let table = resolve_object_storage_key(session, name)?;
                        if session.catalog.is_view(&table) {
                            return execute_view_query(
                                engine, session, &table, order_by, offset, limit, privileges,
                            );
                        }
                        if table == privileges::SHOW_GRANTS_VIRTUAL_TABLE {
                            let account = extract_eq_predicate(select.selection.as_ref())
                                .filter(|(col, _)| col == "__account__")
                                .map(|(_, v)| v)
                                .ok_or_else(|| {
                                    ExecError::Message("SHOW GRANTS requires account".into())
                                })?;
                            let (user, host) = account.split_once('@').map_or_else(
                                || (account.clone(), "%".to_string()),
                                |(user, host)| (user.to_string(), host.to_string()),
                            );
                            return show_grants_result(privileges, &user, &host);
                        }
                        if table == privileges::MYSQL_USER_VIRTUAL_TABLE {
                            return Ok(mysql_user_stub_rows(privileges));
                        }
                        if table == info_schema::SHOW_INDEX_VIRTUAL_TABLE {
                            let table_name = extract_eq_predicate(select.selection.as_ref())
                                .filter(|(col, _)| col == "__table__")
                                .map(|(_, v)| v)
                                .ok_or_else(|| {
                                    ExecError::Message("SHOW INDEX requires a table".into())
                                })?;
                            return info_schema::show_index_for_table(engine, session, &table_name);
                        }
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
                                    session,
                                    &session.database,
                                ),
                                "columns" => info_schema::scan_information_schema_columns(
                                    engine,
                                    session,
                                    table_filter.as_deref(),
                                )?,
                                "schemata" => info_schema::scan_information_schema_schemata(
                                    &engine.list_databases(),
                                ),
                                "statistics" => info_schema::scan_information_schema_statistics(
                                    engine, session,
                                )?,
                                "views" => info_schema::scan_information_schema_views(session),
                                "key_column_usage" => {
                                    info_schema::scan_information_schema_key_column_usage(session)
                                }
                                other => {
                                    return Err(ExecError::Message(format!(
                                        "unsupported information_schema view: {other}"
                                    )))
                                }
                            };
                            return finish_rows_query(result, order_by, offset, limit);
                        }
                        let table_columns: Vec<String> = session
                            .catalog
                            .get_table(&table)
                            .map(|m| m.columns.iter().map(|c| c.name.clone()).collect())
                            .unwrap_or_default();
                        if select_has_group_by(select) {
                            let rows = match parse_where_with_subqueries(select.selection.as_ref())?
                            {
                                None => engine.scan(&table)?,
                                Some(filter) => filter_inline_rows(
                                    engine,
                                    session,
                                    engine.scan(&table)?,
                                    &table_columns,
                                    &filter,
                                )?,
                            };
                            let (columns, rows) = execute_group_by(select, &table_columns, rows)?;
                            let rows = finish_row_set(rows, &columns, order_by, offset, limit)?;
                            return Ok(QueryResult::Rows { columns, rows });
                        }
                        let rows = match parse_where_with_subqueries(select.selection.as_ref())? {
                            None => engine.scan(&table)?,
                            Some(filter) => {
                                let eq_prefix = eq_prefix_from_filter(&filter);
                                if !eq_prefix.is_empty() {
                                    let eq_refs: Vec<(&str, &str)> = eq_prefix
                                        .iter()
                                        .map(|(c, v)| (c.as_str(), v.as_str()))
                                        .collect();
                                    match engine.scan_eq_prefix(&table, &eq_refs)? {
                                        Some(indexed) => filter_inline_rows(
                                            engine,
                                            session,
                                            indexed,
                                            &table_columns,
                                            &filter,
                                        )?,
                                        None => filter_inline_rows(
                                            engine,
                                            session,
                                            engine.scan(&table)?,
                                            &table_columns,
                                            &filter,
                                        )?,
                                    }
                                } else if let Some((ref col, ref val)) =
                                    eq_predicate_from_filter(&filter)
                                {
                                    match engine.scan_eq(&table, col, val)? {
                                        Some(indexed) => indexed,
                                        None => filter_inline_rows(
                                            engine,
                                            session,
                                            engine.scan(&table)?,
                                            &table_columns,
                                            &filter,
                                        )?,
                                    }
                                } else if let Some((ref col, ref low, ref high)) =
                                    between_predicate_from_filter(&filter)
                                {
                                    match engine.scan_range(&table, col, low, high)? {
                                        Some(indexed) => indexed,
                                        None => filter_inline_rows(
                                            engine,
                                            session,
                                            engine.scan(&table)?,
                                            &table_columns,
                                            &filter,
                                        )?,
                                    }
                                } else {
                                    filter_inline_rows(
                                        engine,
                                        session,
                                        engine.scan(&table)?,
                                        &table_columns,
                                        &filter,
                                    )?
                                }
                            }
                        };
                        let (columns, rows) =
                            eval_or_project_select(engine, session, select, table_columns, rows)?;
                        let rows = finish_row_set(rows, &columns, order_by, offset, limit)?;
                        return Ok(QueryResult::Rows { columns, rows });
                    }
                }
                if select.projection.len() == 1 {
                    if let SelectItem::UnnamedExpr(Expr::Identifier(id)) = &select.projection[0] {
                        if id.value.eq_ignore_ascii_case("@@version_comment") {
                            return Ok(QueryResult::Rows {
                                columns: vec!["@@version_comment".into()],
                                rows: vec![vec!["8.0.33-rusql".into()]],
                            });
                        }
                    }
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

fn execute_set_expr<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    expr: &SetExpr,
    privileges: &PrivilegeStore,
) -> Result<(Vec<String>, Vec<Row>), ExecError> {
    match expr {
        SetExpr::SetOperation {
            op,
            set_quantifier,
            left,
            right,
        } => {
            if *op != SetOperator::Union {
                return Err(ExecError::Message(format!(
                    "unsupported set operator: {op}"
                )));
            }
            let union_all = matches!(set_quantifier, SetQuantifier::All);
            let (left_cols, left_rows) = execute_set_expr(engine, session, left, privileges)?;
            let (right_cols, right_rows) = execute_set_expr(engine, session, right, privileges)?;
            if left_cols.len() != right_cols.len() {
                return Err(ExecError::Message("UNION column count mismatch".into()));
            }
            let mut rows = left_rows;
            rows.extend(right_rows);
            if !union_all {
                rows = dedupe_rows(rows);
            }
            Ok((left_cols, rows))
        }
        SetExpr::Query(q) => {
            let result = execute_one(
                engine,
                session,
                &Plan::Statement(Statement::Query(q.clone())),
                privileges,
            )?;
            match result {
                QueryResult::Rows { columns, rows } => Ok((columns, rows)),
                QueryResult::Ok { .. } => Ok((vec![], vec![])),
            }
        }
        SetExpr::Select(select) => {
            let nested = sqlparser::ast::Query {
                with: None,
                body: Box::new(SetExpr::Select(select.clone())),
                order_by: None,
                limit: None,
                limit_by: vec![],
                offset: None,
                fetch: None,
                locks: vec![],
                for_clause: None,
                settings: None,
                format_clause: None,
            };
            let result = execute_one(
                engine,
                session,
                &Plan::Statement(Statement::Query(Box::new(nested))),
                privileges,
            )?;
            match result {
                QueryResult::Rows { columns, rows } => Ok((columns, rows)),
                QueryResult::Ok { .. } => Ok((vec![], vec![])),
            }
        }
        other => Err(ExecError::Message(format!(
            "unsupported set expression: {other:?}"
        ))),
    }
}

fn dedupe_rows(rows: Vec<Row>) -> Vec<Row> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let key = row.join("\x1f");
        if seen.insert(key) {
            out.push(row);
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JoinKind {
    Inner,
    Left,
    Right,
}

struct JoinSide {
    table_name: String,
    alias: String,
    columns: Vec<String>,
}

fn join_side_from_factor(factor: &TableFactor, session: &Session) -> Result<JoinSide, ExecError> {
    let TableFactor::Table { name, alias, .. } = factor else {
        return Err(ExecError::Message("JOIN requires base tables".into()));
    };
    let table_name = resolve_object_storage_key(session, name)?;
    let alias = alias
        .as_ref()
        .map(|a| a.name.value.clone())
        .unwrap_or_else(|| table_name.clone());
    let columns = session
        .catalog
        .get_table(&table_name)
        .map(|m| m.columns.iter().map(|c| c.name.clone()).collect())
        .unwrap_or_default();
    Ok(JoinSide {
        table_name,
        alias,
        columns,
    })
}

pub(crate) fn scan_table_factor<E: StorageEngine>(
    engine: &mut E,
    session: &Session,
    factor: &TableFactor,
) -> Result<(Vec<String>, Vec<Row>), ExecError> {
    let side = join_side_from_factor(factor, session)?;
    let rows = engine.scan(&side.table_name)?;
    Ok((side.columns, rows))
}

struct JoinKey {
    left_qualifier: String,
    left_col: String,
    right_qualifier: String,
    right_col: String,
}

fn qualified_column(expr: &Expr) -> Result<(String, String), ExecError> {
    match expr {
        Expr::CompoundIdentifier(parts) if parts.len() >= 2 => Ok((
            parts[parts.len() - 2].value.clone(),
            parts.last().unwrap().value.clone(),
        )),
        Expr::Identifier(id) => Ok((String::new(), id.value.clone())),
        other => Err(ExecError::Message(format!(
            "unsupported JOIN column: {other:?}"
        ))),
    }
}

fn parse_join_on_eq(expr: &Expr) -> Result<JoinKey, ExecError> {
    let Expr::BinaryOp {
        left,
        op: BinaryOperator::Eq,
        right,
    } = expr
    else {
        return Err(ExecError::Message("JOIN ON requires equality".into()));
    };
    let (left_qualifier, left_col) = qualified_column(left)?;
    let (right_qualifier, right_col) = qualified_column(right)?;
    Ok(JoinKey {
        left_qualifier,
        left_col,
        right_qualifier,
        right_col,
    })
}

fn side_matches(side: &JoinSide, qualifier: &str) -> bool {
    side.alias.eq_ignore_ascii_case(qualifier) || side.table_name.eq_ignore_ascii_case(qualifier)
}

fn null_row(width: usize) -> Row {
    vec![String::new(); width]
}

fn nested_loop_join(
    kind: JoinKind,
    left: &JoinSide,
    left_rows: Vec<Row>,
    right: &JoinSide,
    right_rows: Vec<Row>,
    key: &JoinKey,
) -> Result<(Vec<String>, Vec<Row>), ExecError> {
    let (left_col, right_col) = if side_matches(left, &key.left_qualifier)
        && side_matches(right, &key.right_qualifier)
    {
        (&key.left_col, &key.right_col)
    } else if side_matches(left, &key.right_qualifier) && side_matches(right, &key.left_qualifier) {
        (&key.right_col, &key.left_col)
    } else {
        return Err(ExecError::Message(
            "JOIN ON qualifiers do not match FROM tables".into(),
        ));
    };

    let left_idx = left
        .columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case(left_col))
        .ok_or_else(|| ExecError::Message(format!("unknown column '{left_col}'")))?;
    let right_idx = right
        .columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case(right_col))
        .ok_or_else(|| ExecError::Message(format!("unknown column '{right_col}'")))?;

    let mut combined_columns = left.columns.clone();
    combined_columns.extend(right.columns.clone());

    let rows_match = |lr: &Row, rr: &Row| {
        lr.get(left_idx).map(|v| v.as_str()) == rr.get(right_idx).map(|v| v.as_str())
    };

    let mut out = Vec::new();
    match kind {
        JoinKind::Inner => {
            for lr in left_rows {
                for rr in &right_rows {
                    if rows_match(&lr, rr) {
                        let mut row = lr.clone();
                        row.extend(rr.clone());
                        out.push(row);
                    }
                }
            }
        }
        JoinKind::Left => {
            for lr in left_rows {
                let mut matched = false;
                for rr in &right_rows {
                    if rows_match(&lr, rr) {
                        let mut row = lr.clone();
                        row.extend(rr.clone());
                        out.push(row);
                        matched = true;
                    }
                }
                if !matched {
                    let mut row = lr;
                    row.extend(null_row(right.columns.len()));
                    out.push(row);
                }
            }
        }
        JoinKind::Right => {
            for rr in right_rows {
                let mut matched = false;
                for lr in &left_rows {
                    if rows_match(lr, &rr) {
                        let mut row = lr.clone();
                        row.extend(rr.clone());
                        out.push(row);
                        matched = true;
                    }
                }
                if !matched {
                    let mut row = null_row(left.columns.len());
                    row.extend(rr);
                    out.push(row);
                }
            }
        }
    }
    Ok((combined_columns, out))
}

fn execute_join_select<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    select: &sqlparser::ast::Select,
    from: &sqlparser::ast::TableWithJoins,
    order_by: Option<&OrderBy>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<QueryResult, ExecError> {
    if from.joins.len() != 1 {
        return Err(ExecError::Message("only single JOIN supported".into()));
    }
    let join = &from.joins[0];
    let (kind, on_expr) = match &join.join_operator {
        JoinOperator::Inner(JoinConstraint::On(expr)) => (JoinKind::Inner, expr),
        JoinOperator::LeftOuter(JoinConstraint::On(expr)) => (JoinKind::Left, expr),
        JoinOperator::RightOuter(JoinConstraint::On(expr)) => (JoinKind::Right, expr),
        _ => {
            return Err(ExecError::Message(
                "only INNER/LEFT/RIGHT JOIN ... ON supported".into(),
            ))
        }
    };

    let left = join_side_from_factor(&from.relation, session)?;
    let right = join_side_from_factor(&join.relation, session)?;
    let key = parse_join_on_eq(on_expr)?;

    let left_rows = engine.scan(&left.table_name)?;
    let right_rows = engine.scan(&right.table_name)?;
    let (table_columns, mut rows) =
        nested_loop_join(kind, &left, left_rows, &right, right_rows, &key)?;

    if let Some(filter) = parse_where_with_subqueries(select.selection.as_ref())? {
        rows = filter_inline_rows(engine, session, rows, &table_columns, &filter)?;
    }

    let (columns, rows) = eval_or_project_select(engine, session, select, table_columns, rows)?;
    let rows = finish_row_set(rows, &columns, order_by, offset, limit)?;
    Ok(QueryResult::Rows { columns, rows })
}

fn table_meta_from_create(
    create: &sqlparser::ast::CreateTable,
    default_schema: &str,
) -> Result<TableMeta, ExecError> {
    let parts: Vec<_> = create.name.0.iter().map(|i| i.value.clone()).collect();
    let (schema, table_name) = match parts.as_slice() {
        [t] => (default_schema.to_string(), t.clone()),
        [s, t] => (s.clone(), t.clone()),
        _ => (
            default_schema.to_string(),
            object_name_to_string(&create.name),
        ),
    };
    let mut columns: Vec<ColumnDef> = create.columns.iter().map(column_def_from_ast).collect();
    let mut foreign_keys = Vec::new();

    for constraint in &create.constraints {
        if let TableConstraint::PrimaryKey {
            columns: pk_cols, ..
        } = constraint
        {
            for id in pk_cols {
                if let Some(col) = columns
                    .iter_mut()
                    .find(|c| c.name.eq_ignore_ascii_case(&id.value))
                {
                    col.primary_key = true;
                    col.nullable = false;
                }
            }
        }
        if let Some(fk) = foreign_key_from_constraint(constraint, &schema)? {
            foreign_keys.push(fk);
        }
    }

    let auto_increment_next = if columns.iter().any(|c| c.auto_increment) {
        Some(create.auto_increment_offset.map(|n| n as u64).unwrap_or(1))
    } else {
        None
    };

    Ok(TableMeta {
        name: table_name,
        schema,
        columns,
        auto_increment_next,
        foreign_keys,
    })
}

fn column_def_from_ast(c: &sqlparser::ast::ColumnDef) -> ColumnDef {
    let mut col = ColumnDef::new(
        c.name.value.clone(),
        normalize_column_type(&c.data_type.to_string()),
    );
    for opt in &c.options {
        match &opt.option {
            ColumnOption::NotNull => col.nullable = false,
            ColumnOption::Null => col.nullable = true,
            ColumnOption::Unique { is_primary, .. } if *is_primary => {
                col.primary_key = true;
                col.nullable = false;
            }
            ColumnOption::DialectSpecific(tokens)
                if tokens
                    .iter()
                    .any(|t| t.to_string().eq_ignore_ascii_case("AUTO_INCREMENT")) =>
            {
                col.auto_increment = true;
                col.nullable = false;
            }
            _ => {}
        }
    }
    col
}

fn execute_alter_table<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    name: &ObjectName,
    operations: &[AlterTableOperation],
) -> Result<QueryResult, ExecError> {
    let table = resolve_object_storage_key(session, name)?;
    for op in operations {
        match op {
            AlterTableOperation::AddColumn {
                if_not_exists,
                column_def,
                column_position,
                ..
            } => {
                if column_position.is_some() {
                    return Err(ExecError::Message(
                        "ALTER TABLE column position (FIRST/AFTER) not supported".into(),
                    ));
                }
                let col = column_def_from_ast(column_def);
                if *if_not_exists && catalog_has_column(session, &table, &col.name) {
                    continue;
                }
                match engine.add_column(&table, col.clone()) {
                    Ok(()) => catalog_push_column(session, &table, col)?,
                    Err(StorageError::Message(msg))
                        if *if_not_exists && msg.contains("duplicate column") => {}
                    Err(e) => return Err(e.into()),
                }
            }
            AlterTableOperation::DropColumn {
                column_name,
                if_exists,
                ..
            } => {
                engine.drop_column(&table, &column_name.value, *if_exists)?;
                catalog_drop_column(session, &table, &column_name.value, *if_exists)?;
            }
            AlterTableOperation::RenameColumn {
                old_column_name,
                new_column_name,
            } => {
                engine.rename_column(&table, &old_column_name.value, &new_column_name.value)?;
                catalog_rename_column(
                    session,
                    &table,
                    &old_column_name.value,
                    &new_column_name.value,
                )?;
            }
            AlterTableOperation::ModifyColumn {
                col_name,
                data_type,
                options,
                column_position,
            } => {
                if column_position.is_some() {
                    return Err(ExecError::Message(
                        "ALTER TABLE column position (FIRST/AFTER) not supported".into(),
                    ));
                }
                let col = column_from_modify(&col_name.value, data_type, options);
                engine.modify_column(&table, col.clone())?;
                catalog_replace_column(session, &table, col)?;
            }
            AlterTableOperation::ChangeColumn {
                old_name,
                new_name,
                data_type,
                options,
                column_position,
            } => {
                if column_position.is_some() {
                    return Err(ExecError::Message(
                        "ALTER TABLE column position (FIRST/AFTER) not supported".into(),
                    ));
                }
                if old_name.value != new_name.value {
                    engine.rename_column(&table, &old_name.value, &new_name.value)?;
                    catalog_rename_column(session, &table, &old_name.value, &new_name.value)?;
                }
                let col = column_from_modify(&new_name.value, data_type, options);
                engine.modify_column(&table, col.clone())?;
                catalog_replace_column(session, &table, col)?;
            }
            AlterTableOperation::RenameTable { table_name } => {
                let new_name = object_name_to_string(table_name);
                engine.rename_table(&table, &new_name)?;
                catalog_rename_table(session, &table, &new_name)?;
            }
            other => {
                return Err(ExecError::Message(format!(
                    "unsupported ALTER TABLE operation: {other}"
                )))
            }
        }
    }
    Ok(QueryResult::Ok { rows_affected: 0 })
}

fn catalog_has_column(session: &Session, table: &str, column: &str) -> bool {
    session.catalog.get_table(table).is_some_and(|meta| {
        meta.columns
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(column))
    })
}

fn catalog_push_column(
    session: &mut Session,
    table: &str,
    column: ColumnDef,
) -> Result<(), ExecError> {
    let mut meta =
        session.catalog.get_table(table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(table))
        })?;
    meta.columns.push(column);
    session.catalog.create_table(meta);
    Ok(())
}

fn column_from_modify(
    name: &str,
    data_type: &sqlparser::ast::DataType,
    options: &[ColumnOption],
) -> ColumnDef {
    let mut col = ColumnDef::new(name, data_type.to_string());
    for opt in options {
        match opt {
            ColumnOption::NotNull => col.nullable = false,
            ColumnOption::Null => col.nullable = true,
            ColumnOption::Unique { is_primary, .. } if *is_primary => {
                col.primary_key = true;
                col.nullable = false;
            }
            _ => {}
        }
    }
    col
}

fn catalog_drop_column(
    session: &mut Session,
    table: &str,
    column: &str,
    if_exists: bool,
) -> Result<(), ExecError> {
    let mut meta =
        session.catalog.get_table(table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(table))
        })?;
    let Some(idx) = meta
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(column))
    else {
        if if_exists {
            return Ok(());
        }
        return Err(ExecError::Message(format!("column '{column}' not found")));
    };
    meta.columns.remove(idx);
    session.catalog.create_table(meta);
    Ok(())
}

fn catalog_rename_column(
    session: &mut Session,
    table: &str,
    old_name: &str,
    new_name: &str,
) -> Result<(), ExecError> {
    let mut meta =
        session.catalog.get_table(table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(table))
        })?;
    let col = meta
        .columns
        .iter_mut()
        .find(|c| c.name.eq_ignore_ascii_case(old_name))
        .ok_or_else(|| ExecError::Message(format!("column '{old_name}' not found")))?;
    col.name = new_name.to_string();
    session.catalog.create_table(meta);
    Ok(())
}

fn catalog_replace_column(
    session: &mut Session,
    table: &str,
    column: ColumnDef,
) -> Result<(), ExecError> {
    let mut meta =
        session.catalog.get_table(table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(table))
        })?;
    let col = meta
        .columns
        .iter_mut()
        .find(|c| c.name.eq_ignore_ascii_case(&column.name))
        .ok_or_else(|| ExecError::Message(format!("column '{}' not found", column.name)))?;
    *col = column;
    session.catalog.create_table(meta);
    Ok(())
}

fn catalog_rename_table(
    session: &mut Session,
    old_name: &str,
    new_name: &str,
) -> Result<(), ExecError> {
    let mut meta = session
        .catalog
        .get_table(old_name)
        .cloned()
        .ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(old_name))
        })?;
    session.catalog.drop_table(old_name);
    meta.name = new_name.to_string();
    session.catalog.create_table(meta);
    Ok(())
}

fn object_name_to_string(name: &ObjectName) -> String {
    name.0
        .iter()
        .map(|i| i.value.clone())
        .collect::<Vec<_>>()
        .join(".")
}

/// Resolve `db.table` or bare `table` (using session default schema) to a storage key.
fn resolve_object_storage_key(session: &Session, name: &ObjectName) -> Result<String, ExecError> {
    let parts: Vec<_> = name.0.iter().map(|i| i.value.as_str()).collect();
    match parts.as_slice() {
        [table] => Ok(table_storage_key(&session.database, table)),
        [schema, table] => Ok(table_storage_key(schema, table)),
        other => Err(ExecError::Message(format!(
            "unsupported table name: {}",
            other.join(".")
        ))),
    }
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

fn delete_table_object_name(delete: &sqlparser::ast::Delete) -> Result<ObjectName, ExecError> {
    let tables = match &delete.from {
        FromTable::WithFromKeyword(t) | FromTable::WithoutKeyword(t) => t,
    };
    let first = tables
        .first()
        .ok_or_else(|| ExecError::Message("DELETE requires a table".into()))?;
    match &first.relation {
        TableFactor::Table { name, .. } => Ok(name.clone()),
        other => Err(ExecError::Message(format!(
            "unsupported DELETE target: {other:?}"
        ))),
    }
}

fn resolve_table_with_joins(
    session: &Session,
    table: &sqlparser::ast::TableWithJoins,
) -> Result<String, ExecError> {
    match &table.relation {
        TableFactor::Table { name, .. } => resolve_object_storage_key(session, name),
        other => Err(ExecError::Message(format!(
            "unsupported UPDATE target: {other:?}"
        ))),
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

/// Expand INSERT values to full-width row; assign AUTO_INCREMENT when omitted.
/// Returns `(row, Some(next_counter))` when the AI counter was consumed/bumped.
fn expand_insert_row(
    meta: &TableMeta,
    columns: &[sqlparser::ast::Ident],
    values: Vec<String>,
    next_ai: Option<u64>,
) -> Result<(Row, Option<u64>), ExecError> {
    let target_indices: Vec<usize> = if columns.is_empty() {
        (0..meta.columns.len()).collect()
    } else {
        columns
            .iter()
            .map(|id| {
                meta.columns
                    .iter()
                    .position(|c| c.name.eq_ignore_ascii_case(&id.value))
                    .ok_or_else(|| {
                        ExecError::Message(format!("Unknown column '{}' in INSERT", id.value))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    if values.len() != target_indices.len() {
        return Err(ExecError::Message(format!(
            "Column count doesn't match value count: {} vs {}",
            target_indices.len(),
            values.len()
        )));
    }

    let mut row = vec![String::new(); meta.columns.len()];
    let mut provided = vec![false; meta.columns.len()];
    for (idx, val) in target_indices.into_iter().zip(values) {
        row[idx] = val;
        provided[idx] = true;
    }

    let mut bumped = next_ai;
    for (i, col) in meta.columns.iter().enumerate() {
        if !col.auto_increment {
            continue;
        }
        let needs_ai = !provided[i] || row[i].is_empty();
        if needs_ai {
            let n = bumped.ok_or_else(|| {
                ExecError::Message(format!(
                    "no AUTO_INCREMENT counter for column '{}'",
                    col.name
                ))
            })?;
            row[i] = n.to_string();
            bumped = Some(n + 1);
        } else if let Ok(v) = row[i].parse::<u64>() {
            let cur = bumped.unwrap_or(1);
            if v >= cur {
                bumped = Some(v + 1);
            }
        }
    }
    Ok((row, bumped))
}

/// Stored representation of SQL NULL in heap rows (empty string).
fn sql_null() -> String {
    String::new()
}

fn expr_to_string(expr: &Expr) -> Result<String, ExecError> {
    match expr {
        Expr::Value(Value::Null) => Ok(sql_null()),
        Expr::Value(Value::Number(n, _)) => Ok(n.clone()),
        Expr::Value(Value::SingleQuotedString(s)) => Ok(s.clone()),
        other => Err(ExecError::Message(format!("unsupported expr: {other:?}"))),
    }
}

fn execute_derived_select<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    select: &sqlparser::ast::Select,
    subquery: &sqlparser::ast::Query,
    order_by: Option<&OrderBy>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<QueryResult, ExecError> {
    let (table_columns, mut rows) = select_from_subquery(engine, session, subquery.clone())?;
    if select_has_group_by(select) {
        if let Some(filter) = parse_where_with_subqueries(select.selection.as_ref())? {
            rows = filter_inline_rows(engine, session, rows, &table_columns, &filter)?;
        }
        let (columns, rows) = execute_group_by(select, &table_columns, rows)?;
        let rows = finish_row_set(rows, &columns, order_by, offset, limit)?;
        return Ok(QueryResult::Rows { columns, rows });
    }
    if let Some(filter) = parse_where_with_subqueries(select.selection.as_ref())? {
        rows = filter_inline_rows(engine, session, rows, &table_columns, &filter)?;
    }
    let (columns, rows) = eval_or_project_select(engine, session, select, table_columns, rows)?;
    let rows = finish_row_set(rows, &columns, order_by, offset, limit)?;
    Ok(QueryResult::Rows { columns, rows })
}

fn eval_or_project_select<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    select: &sqlparser::ast::Select,
    table_columns: Vec<String>,
    rows: Vec<Row>,
) -> Result<(Vec<String>, Vec<Row>), ExecError> {
    if projection_needs_eval(&select.projection) {
        eval_projection_select(engine, session, select, &table_columns, rows)
    } else {
        let (out_columns, proj_indices) = resolve_projection(&select.projection, &table_columns)?;
        finalize_select_rows(out_columns, proj_indices, table_columns, rows)
    }
}

fn projection_needs_eval(projection: &[SelectItem]) -> bool {
    projection.iter().any(|item| match item {
        SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => false,
        SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
            !matches!(expr, Expr::Identifier(_) | Expr::CompoundIdentifier(_))
        }
    })
}

fn eval_projection_select<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    select: &sqlparser::ast::Select,
    table_columns: &[String],
    rows: Vec<Row>,
) -> Result<(Vec<String>, Vec<Row>), ExecError> {
    if select.projection.len() == 1 {
        match &select.projection[0] {
            SelectItem::Wildcard(_) => {
                return Ok((table_columns.to_vec(), rows));
            }
            SelectItem::QualifiedWildcard(prefix, _) => {
                let prefix = object_name_to_string(prefix);
                let cols: Vec<String> = table_columns
                    .iter()
                    .filter(|c| c.starts_with(&format!("{prefix}.")))
                    .cloned()
                    .collect();
                let indices: Vec<usize> = cols
                    .iter()
                    .map(|c| column_index(table_columns, c))
                    .collect::<Result<_, _>>()?;
                let out_rows = rows
                    .into_iter()
                    .map(|row| indices.iter().map(|i| row[*i].clone()).collect())
                    .collect();
                return Ok((cols, out_rows));
            }
            _ => {}
        }
    }
    let mut out_columns = Vec::with_capacity(select.projection.len());
    for item in &select.projection {
        let (expr, alias) = match item {
            SelectItem::UnnamedExpr(expr) => (expr, None),
            SelectItem::ExprWithAlias { expr, alias } => (expr, Some(alias.value.as_str())),
            other => {
                return Err(ExecError::Message(format!(
                    "unsupported SELECT item: {other:?}"
                )))
            }
        };
        out_columns.push(expr_output_name(expr, alias)?);
    }
    let mut out_rows = Vec::with_capacity(rows.len());
    for row in rows {
        let mut out_row = Vec::with_capacity(select.projection.len());
        for item in &select.projection {
            let (expr, _alias) = match item {
                SelectItem::UnnamedExpr(expr) => (expr, None),
                SelectItem::ExprWithAlias { expr, alias } => (expr, Some(alias.value.as_str())),
                other => {
                    return Err(ExecError::Message(format!(
                        "unsupported SELECT item: {other:?}"
                    )))
                }
            };
            let val = match expr {
                Expr::Subquery(q) => eval_scalar_subquery(engine, session, *q.clone())?,
                other => eval_expr(&row, table_columns, other)?,
            };
            out_row.push(val);
        }
        out_rows.push(out_row);
    }
    Ok((out_columns, out_rows))
}

fn column_index(columns: &[String], name: &str) -> Result<usize, ExecError> {
    columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case(name))
        .ok_or_else(|| ExecError::Message(format!("unknown column '{name}'")))
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

fn extract_offset(offset: Option<&Offset>) -> Result<Option<usize>, ExecError> {
    let Some(off) = offset else {
        return Ok(None);
    };
    match &off.value {
        Expr::Value(Value::Number(n, _)) => n
            .parse::<usize>()
            .map(Some)
            .map_err(|_| ExecError::Message("invalid OFFSET value".into())),
        other => Err(ExecError::Message(format!("unsupported OFFSET: {other:?}"))),
    }
}

fn extract_limit(limit: Option<&Expr>) -> Result<Option<usize>, ExecError> {
    let Some(expr) = limit else {
        return Ok(None);
    };
    match expr {
        Expr::Value(Value::Number(n, _)) => n
            .parse::<usize>()
            .map(Some)
            .map_err(|_| ExecError::Message("invalid LIMIT value".into())),
        other => Err(ExecError::Message(format!("unsupported LIMIT: {other:?}"))),
    }
}

struct SortKey {
    col_idx: usize,
    ascending: bool,
}

fn resolve_order_by(
    order_by: Option<&OrderBy>,
    columns: &[String],
) -> Result<Vec<SortKey>, ExecError> {
    let Some(order_by) = order_by else {
        return Ok(vec![]);
    };
    let mut keys = Vec::with_capacity(order_by.exprs.len());
    for ob in &order_by.exprs {
        if ob.nulls_first.is_some() {
            return Err(ExecError::Message(
                "NULLS FIRST/LAST in ORDER BY not supported".into(),
            ));
        }
        let col = expr_column_name(&ob.expr)?;
        let idx = columns
            .iter()
            .position(|c| c.eq_ignore_ascii_case(&col))
            .ok_or_else(|| ExecError::Message(format!("unknown ORDER BY column '{col}'")))?;
        keys.push(SortKey {
            col_idx: idx,
            ascending: ob.asc.unwrap_or(true),
        });
    }
    Ok(keys)
}

fn apply_order_by(mut rows: Vec<Row>, keys: &[SortKey]) -> Vec<Row> {
    if keys.is_empty() {
        return rows;
    }
    rows.sort_by(|a, b| {
        for key in keys {
            let va = a.get(key.col_idx).map(|s| s.as_str()).unwrap_or("");
            let vb = b.get(key.col_idx).map(|s| s.as_str()).unwrap_or("");
            let ord = va.cmp(vb);
            let ord = if key.ascending { ord } else { ord.reverse() };
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        std::cmp::Ordering::Equal
    });
    rows
}

fn finish_row_set(
    rows: Vec<Row>,
    columns: &[String],
    order_by: Option<&OrderBy>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<Vec<Row>, ExecError> {
    let keys = resolve_order_by(order_by, columns)?;
    Ok(apply_pagination(apply_order_by(rows, &keys), offset, limit))
}

fn finish_rows_query(
    result: QueryResult,
    order_by: Option<&OrderBy>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<QueryResult, ExecError> {
    match result {
        QueryResult::Rows { columns, rows } => {
            let rows = finish_row_set(rows, &columns, order_by, offset, limit)?;
            Ok(QueryResult::Rows { columns, rows })
        }
        other => Ok(other),
    }
}

fn execute_view_query<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    view_name: &str,
    order_by: Option<&OrderBy>,
    offset: Option<usize>,
    limit: Option<usize>,
    privileges: &PrivilegeStore,
) -> Result<QueryResult, ExecError> {
    let view = session
        .catalog
        .get_view(view_name)
        .cloned()
        .ok_or_else(|| ExecError::Message(format!("view '{view_name}' not found")))?;
    let stmts = rusql_sql::parse(&view.sql).map_err(|e| ExecError::Message(e.to_string()))?;
    let stmt = stmts
        .into_iter()
        .next()
        .ok_or_else(|| ExecError::Message("empty view definition".into()))?;
    let Statement::Query(view_query) = stmt else {
        return Err(ExecError::Message(
            "view definition must be a SELECT query".into(),
        ));
    };
    let result = execute_one(
        engine,
        session,
        &Plan::Statement(Statement::Query(view_query)),
        privileges,
    )?;
    finish_rows_query(result, order_by, offset, limit)
}

fn apply_pagination(rows: Vec<Row>, offset: Option<usize>, limit: Option<usize>) -> Vec<Row> {
    let rows = match offset {
        Some(n) => rows.into_iter().skip(n).collect(),
        None => rows,
    };
    apply_limit(rows, limit)
}

fn apply_limit(rows: Vec<Row>, limit: Option<usize>) -> Vec<Row> {
    match limit {
        Some(n) => rows.into_iter().take(n).collect(),
        None => rows,
    }
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
    fn describe_primary_key_and_not_null() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        let create =
            parse("CREATE TABLE pk_t (id INT PRIMARY KEY, label VARCHAR(16) NOT NULL)").unwrap();
        let plans = plan(&session, create);
        exec.execute(&mut session, &plans, None).unwrap();

        let describe = parse("DESCRIBE pk_t").unwrap();
        let plans = plan(&session, describe);
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0][2], "NO");
                assert_eq!(rows[0][3], "PRI");
                assert_eq!(rows[1][2], "NO");
                assert_eq!(rows[1][3], "");
            }
            _ => panic!("expected rows"),
        }

        let show = parse("SHOW CREATE TABLE pk_t").unwrap();
        let plans = plan(&session, show);
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert!(rows[0][1].contains("PRIMARY KEY"));
                assert!(rows[0][1].contains("NOT NULL"));
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn create_insert_and_select() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        let create = parse("CREATE TABLE t (id INT)").unwrap();
        let plans = plan(&session, create);
        exec.execute(&mut session, &plans, None).unwrap();

        let insert = parse("INSERT INTO t VALUES (1)").unwrap();
        let plans = plan(&session, insert);
        let results = exec.execute(&mut session, &plans, None).unwrap();
        assert_eq!(results[0], QueryResult::Ok { rows_affected: 1 });

        let select = parse("SELECT * FROM t").unwrap();
        let plans = plan(&session, select);
        let results = exec.execute(&mut session, &plans, None).unwrap();
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
            exec.execute(&mut session, &plans, None).unwrap();
        }

        let select = parse("SELECT * FROM t WHERE id = 2").unwrap();
        let plans = plan(&session, select);
        let results = exec.execute(&mut session, &plans, None).unwrap();
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
        exec.execute(&mut session, &plans, None).unwrap();

        let show = parse("SHOW TABLES").unwrap();
        let plans = plan(&session, show);
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["Tables_in_rusql".to_string()]);
                assert_eq!(rows, &vec![vec!["items".to_string()]]);
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn extended_column_types_describe() {
        let meta = TableMeta {
            name: "types_t".into(),
            schema: "rusql".into(),
            columns: vec![
                ColumnDef::new("amount", "DECIMAL(10,2)"),
                ColumnDef::new("created_at", "DATETIME"),
                ColumnDef::new("payload", "JSON"),
            ],
            auto_increment_next: None,
            ..Default::default()
        };
        match info_schema::describe_table(&meta) {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows[0][1], "decimal(10,2)");
                assert_eq!(rows[1][1], "datetime");
                assert_eq!(rows[2][1], "json");
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
        exec.execute(&mut session, &plans, None).unwrap();

        let show = parse("SHOW CREATE TABLE items").unwrap();
        let plans = plan(&session, show);
        let results = exec.execute(&mut session, &plans, None).unwrap();
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
        exec.execute(&mut session, &plans, None).unwrap();

        for sql in ["DESCRIBE users", "SHOW COLUMNS FROM users"] {
            let stmts = parse(sql).unwrap();
            let plans = plan(&session, stmts);
            let results = exec.execute(&mut session, &plans, None).unwrap();
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
    fn select_limit() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE t (id INT, name VARCHAR(8))",
            "INSERT INTO t VALUES (1, 'a')",
            "INSERT INTO t VALUES (2, 'b')",
            "INSERT INTO t VALUES (3, 'c')",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }

        let plans = plan(&session, parse("SELECT * FROM t LIMIT 2").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => assert_eq!(rows.len(), 2),
            _ => panic!("expected rows"),
        }

        let plans = plan(
            &session,
            parse("SELECT name FROM t WHERE id = 3 LIMIT 1").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["name".to_string()]);
                assert_eq!(rows, &vec![vec!["c".to_string()]]);
            }
            _ => panic!("expected rows"),
        }

        let plans = plan(
            &session,
            parse("SELECT * FROM t ORDER BY id LIMIT 2 OFFSET 1").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    &vec![
                        vec!["2".to_string(), "b".to_string()],
                        vec!["3".to_string(), "c".to_string()],
                    ]
                );
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn select_order_by() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE t (id INT, name VARCHAR(8))",
            "INSERT INTO t VALUES (2, 'b')",
            "INSERT INTO t VALUES (1, 'a')",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }

        let plans = plan(&session, parse("SELECT * FROM t ORDER BY id").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    &vec![
                        vec!["1".to_string(), "a".to_string()],
                        vec!["2".to_string(), "b".to_string()],
                    ]
                );
            }
            _ => panic!("expected rows"),
        }

        let plans = plan(
            &session,
            parse("SELECT name FROM t ORDER BY name DESC").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["name".to_string()]);
                assert_eq!(rows, &vec![vec!["b".to_string()], vec!["a".to_string()]]);
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn use_database() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();

        let plans = plan(&session, parse("USE rusql").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        assert_eq!(results[0], QueryResult::Ok { rows_affected: 0 });
        assert_eq!(session.database, "rusql");

        let plans = plan(&session, parse("USE unknown_db").unwrap());
        assert!(exec.execute(&mut session, &plans, None).is_err());
    }

    #[test]
    fn create_drop_database_and_use() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();

        for sql in [
            "CREATE DATABASE app_db",
            "USE app_db",
            "CREATE TABLE t (id INT)",
            "INSERT INTO t VALUES (7)",
            "SHOW DATABASES",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            let results = exec.execute(&mut session, &plans, None).unwrap();
            if sql == "SHOW DATABASES" {
                match &results[0] {
                    QueryResult::Rows { rows, .. } => {
                        assert!(rows.iter().any(|r| r[0] == "app_db"));
                        assert!(rows.iter().any(|r| r[0] == "rusql"));
                    }
                    _ => panic!("expected rows"),
                }
            }
        }
        assert_eq!(session.database, "app_db");

        let plans = plan(&session, parse("SELECT * FROM t").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows, &vec![vec!["7".to_string()]]);
            }
            _ => panic!("expected rows"),
        }

        // Non-empty database cannot be dropped.
        let plans = plan(&session, parse("DROP DATABASE app_db").unwrap());
        assert!(exec.execute(&mut session, &plans, None).is_err());

        let plans = plan(&session, parse("DROP TABLE t").unwrap());
        exec.execute(&mut session, &plans, None).unwrap();
        let plans = plan(&session, parse("DROP DATABASE app_db").unwrap());
        exec.execute(&mut session, &plans, None).unwrap();
        assert_eq!(session.database, "rusql");
    }

    #[test]
    fn auto_increment_insert_and_show_create() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE ai_t (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(16))",
            "INSERT INTO ai_t (name) VALUES ('alice')",
            "INSERT INTO ai_t (name) VALUES ('bob')",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }

        let plans = plan(&session, parse("SELECT id, name FROM ai_t").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    &vec![
                        vec!["1".to_string(), "alice".to_string()],
                        vec!["2".to_string(), "bob".to_string()],
                    ]
                );
            }
            _ => panic!("expected rows"),
        }

        let plans = plan(&session, parse("SHOW CREATE TABLE ai_t").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert!(rows[0][1].contains("AUTO_INCREMENT"));
                assert!(rows[0][1].contains("AUTO_INCREMENT=3"));
            }
            _ => panic!("expected rows"),
        }
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
            exec.execute(&mut session, &plans, None).unwrap();
        }

        let plans = plan(&session, parse("SELECT name FROM users").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
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
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["id".to_string(), "name".to_string()]);
                assert_eq!(rows, &vec![vec!["2".to_string(), "bob".to_string()]]);
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn select_column_aliases() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE users (id INT, name VARCHAR(32))",
            "INSERT INTO users VALUES (1, 'alice')",
            "INSERT INTO users VALUES (2, 'bob')",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }

        let plans = plan(&session, parse("SELECT id AS user_id FROM users").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["user_id".to_string()]);
                assert_eq!(rows, &vec![vec!["1".to_string()], vec!["2".to_string()]]);
            }
            _ => panic!("expected rows"),
        }

        let plans = plan(
            &session,
            parse("SELECT id AS user_id, name AS display_name FROM users WHERE id = 2").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    &vec!["user_id".to_string(), "display_name".to_string()]
                );
                assert_eq!(rows, &vec![vec!["2".to_string(), "bob".to_string()]]);
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn where_comparisons_and() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE scores (id INT, name VARCHAR(8))",
            "INSERT INTO scores VALUES (1, 'a')",
            "INSERT INTO scores VALUES (2, 'b')",
            "INSERT INTO scores VALUES (3, 'c')",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }

        let plans = plan(
            &session,
            parse("SELECT * FROM scores WHERE id >= 2").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => assert_eq!(rows.len(), 2),
            _ => panic!("expected rows"),
        }

        let plans = plan(
            &session,
            parse("SELECT name FROM scores WHERE id = 2 AND name = 'b'").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["name".to_string()]);
                assert_eq!(rows, &vec![vec!["b".to_string()]]);
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn inner_join_two_tables() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE orders (id INT, customer VARCHAR(8))",
            "CREATE TABLE order_items (order_id INT, sku VARCHAR(8))",
            "INSERT INTO orders VALUES (1, 'a')",
            "INSERT INTO order_items VALUES (1, 'x')",
            "INSERT INTO order_items VALUES (2, 'y')",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }

        let plans = plan(
            &session,
            parse(
                "SELECT orders.id, order_items.sku FROM orders INNER JOIN order_items ON orders.id = order_items.order_id",
            )
            .unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["id".to_string(), "sku".to_string()]);
                assert_eq!(rows, &vec![vec!["1".to_string(), "x".to_string()]]);
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn union_deduplicates_and_union_all_preserves() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE u_a (id INT, tag VARCHAR(8))",
            "CREATE TABLE u_b (id INT, tag VARCHAR(8))",
            "INSERT INTO u_a VALUES (1, 'x'), (2, 'y')",
            "INSERT INTO u_b VALUES (2, 'y'), (3, 'z')",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }
        let plans = plan(
            &session,
            parse("SELECT id, tag FROM u_a UNION SELECT id, tag FROM u_b ORDER BY id, tag")
                .unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows.len(), 3);
            }
            _ => panic!("expected rows"),
        }
        let plans = plan(
            &session,
            parse("SELECT id FROM u_a UNION ALL SELECT id FROM u_b ORDER BY id").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows.len(), 4);
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn left_outer_join_null_pads_unmatched() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE oj_a (id INT, name VARCHAR(8))",
            "CREATE TABLE oj_b (a_id INT, label VARCHAR(8))",
            "INSERT INTO oj_a VALUES (1, 'alice')",
            "INSERT INTO oj_a VALUES (2, 'bob')",
            "INSERT INTO oj_b VALUES (1, 'x')",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }
        let plans = plan(
            &session,
            parse(
                "SELECT oj_a.id, oj_b.label FROM oj_a LEFT JOIN oj_b ON oj_a.id = oj_b.a_id ORDER BY oj_a.id",
            )
            .unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    &vec![
                        vec!["1".to_string(), "x".to_string()],
                        vec!["2".to_string(), String::new()],
                    ]
                );
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn where_is_null() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE nullable_t (id INT, note VARCHAR(16))",
            "INSERT INTO nullable_t VALUES (1, NULL)",
            "INSERT INTO nullable_t VALUES (2, 'ok')",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }

        let plans = plan(
            &session,
            parse("SELECT id FROM nullable_t WHERE note IS NULL").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["id".to_string()]);
                assert_eq!(rows, &vec![vec!["1".to_string()]]);
            }
            _ => panic!("expected rows"),
        }

        let plans = plan(
            &session,
            parse("SELECT id FROM nullable_t WHERE note IS NOT NULL").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows, &vec![vec!["2".to_string()]]);
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn alter_table_add_column() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE alter_t (id INT)",
            "INSERT INTO alter_t VALUES (1)",
            "ALTER TABLE alter_t ADD COLUMN note VARCHAR(16)",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }

        let plans = plan(&session, parse("SELECT id, note FROM alter_t").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["id".to_string(), "note".to_string()]);
                assert_eq!(rows, &vec![vec!["1".to_string(), "".to_string()]]);
            }
            _ => panic!("expected rows"),
        }

        let plans = plan(&session, parse("DESCRIBE alter_t").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[1][0], "note");
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn alter_table_add_column_mysql_shorthand() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE alter_t2 (id INT)",
            "ALTER TABLE alter_t2 ADD score INT",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }
        let meta = session.catalog.get_table("alter_t2").unwrap();
        assert_eq!(meta.columns.len(), 2);
        assert_eq!(meta.columns[1].name, "score");
    }

    #[test]
    fn alter_table_drop_rename_modify_and_rename_table() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE alt (id INT, extra VARCHAR(8), name VARCHAR(16))",
            "INSERT INTO alt VALUES (1, 'x', 'alice')",
            "ALTER TABLE alt DROP COLUMN extra",
            "ALTER TABLE alt RENAME COLUMN name TO label",
            "ALTER TABLE alt MODIFY COLUMN label VARCHAR(32) NOT NULL",
            "ALTER TABLE alt RENAME TO alt2",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }
        assert!(session.catalog.get_table("alt").is_none());
        let meta = session.catalog.get_table("alt2").unwrap();
        assert_eq!(meta.columns.len(), 2);
        assert_eq!(meta.columns[1].name, "label");
        assert!(!meta.columns[1].nullable);

        let plans = plan(&session, parse("SELECT * FROM alt2").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["id".to_string(), "label".to_string()]);
                assert_eq!(rows, &vec![vec!["1".to_string(), "alice".to_string()]]);
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
        exec.execute(&mut session, &plans, None).unwrap();

        let tables = parse("SELECT * FROM information_schema.tables").unwrap();
        let plans = plan(&session, tables);
        let results = exec.execute(&mut session, &plans, None).unwrap();
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
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][2], "id");
                assert_eq!(rows[0][4], "int");
                assert_eq!(rows[0][7], "utf8mb4_unicode_ci");
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn create_view_and_select() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE vt (id INT, label VARCHAR(16))",
            "INSERT INTO vt VALUES (1, 'a')",
            "CREATE VIEW v_ids AS SELECT id FROM vt",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }
        let plans = plan(&session, parse("SELECT * FROM v_ids").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["id".to_string()]);
                assert_eq!(rows, &vec![vec!["1".to_string()]]);
            }
            other => panic!("expected rows, got {other:?}"),
        }

        let plans = plan(
            &session,
            parse("SELECT * FROM information_schema.VIEWS").unwrap(),
        );
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns[0], "TABLE_SCHEMA");
                assert_eq!(columns[2], "VIEW_DEFINITION");
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][1], "v_ids");
                assert!(rows[0][2].contains("SELECT id FROM vt"));
            }
            other => panic!("expected views rows, got {other:?}"),
        }
    }

    #[test]
    fn show_index_from_table() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE idx_show (id INT PRIMARY KEY, name VARCHAR(16))",
            "CREATE INDEX idx_show_name ON idx_show (name)",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }
        let plans = plan(&session, parse("SHOW INDEX FROM idx_show").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns[2], "Key_name");
                assert_eq!(columns[3], "Seq_in_index");
                assert_eq!(columns[4], "Column_name");
                assert_eq!(
                    rows,
                    &vec![
                        vec![
                            "idx_show".to_string(),
                            "0".to_string(),
                            "PRIMARY".to_string(),
                            "1".to_string(),
                            "id".to_string(),
                            "BTREE".to_string(),
                        ],
                        vec![
                            "idx_show".to_string(),
                            "1".to_string(),
                            "idx_show_name".to_string(),
                            "1".to_string(),
                            "name".to_string(),
                            "BTREE".to_string(),
                        ],
                    ]
                );
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn foreign_key_insert_and_delete_restrict() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE fk_parent (id INT PRIMARY KEY)",
            "CREATE TABLE fk_child (id INT PRIMARY KEY, parent_id INT, CONSTRAINT fk_c_p FOREIGN KEY (parent_id) REFERENCES fk_parent (id))",
            "INSERT INTO fk_parent VALUES (1)",
            "INSERT INTO fk_child VALUES (1, 1)",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }
        let bad = plan(
            &session,
            parse("INSERT INTO fk_child VALUES (2, 9)").unwrap(),
        );
        assert!(matches!(
            exec.execute(&mut session, &bad, None).unwrap_err(),
            ExecError::Mysql { code: 1452, .. }
        ));
        let del = plan(
            &session,
            parse("DELETE FROM fk_parent WHERE id = 1").unwrap(),
        );
        assert!(matches!(
            exec.execute(&mut session, &del, None).unwrap_err(),
            ExecError::Mysql { code: 1451, .. }
        ));
    }
}

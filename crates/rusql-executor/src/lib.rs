//! Query executor for rusql.

mod info_schema;

use rusql_core::{ColumnDef, IndexMeta, Session, TableMeta};
use rusql_planner::Plan;
use rusql_storage::{ColumnAssignment, DeleteFilter, HeapEngine, Row, StorageEngine};
use sqlparser::ast::{
    Assignment, AssignmentTarget, BinaryOperator, ColumnOption, DescribeAlias, Expr, FromTable,
    JoinConstraint, JoinOperator, ObjectName, ObjectType, Offset, OrderBy, SelectItem, SetExpr,
    ShowCreateObject, Statement, TableConstraint, TableFactor, Use, Value,
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
            let meta = table_meta_from_create(create);
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
            let limit = extract_limit(query.limit.as_ref())?;
            let offset = extract_offset(query.offset.as_ref())?;
            let order_by = query.order_by.as_ref();
            if let SetExpr::Select(select) = query.body.as_ref() {
                if let Some(from) = select.from.first() {
                    if !from.joins.is_empty() {
                        return execute_join_select(
                            engine, session, select, from, order_by, offset, limit,
                        );
                    }
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
                            return finish_rows_query(result, order_by, offset, limit);
                        }
                        let table_columns: Vec<String> = session
                            .catalog
                            .get_table(&table)
                            .map(|m| m.columns.iter().map(|c| c.name.clone()).collect())
                            .unwrap_or_default();
                        let (out_columns, proj_indices) =
                            resolve_projection(&select.projection, &table_columns)?;
                        let rows = match parse_where_filter(select.selection.as_ref())? {
                            None => engine.scan(&table)?,
                            Some(WhereFilter::Pred(Predicate::Compare(pred)))
                                if pred.op == CompareOp::Eq =>
                            {
                                match engine.scan_eq(&table, &pred.column, &pred.value)? {
                                    Some(indexed) => indexed,
                                    None => filter_rows(
                                        engine.scan(&table)?,
                                        &table_columns,
                                        &WhereFilter::Pred(Predicate::Compare(pred)),
                                    )?,
                                }
                            }
                            Some(filter) => {
                                filter_rows(engine.scan(&table)?, &table_columns, &filter)?
                            }
                        };
                        let (columns, rows) =
                            finalize_select_rows(out_columns, proj_indices, table_columns, rows)?;
                        let rows = finish_row_set(rows, &columns, order_by, offset, limit)?;
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

struct JoinSide {
    table_name: String,
    alias: String,
    columns: Vec<String>,
}

fn join_side_from_factor(factor: &TableFactor, session: &Session) -> Result<JoinSide, ExecError> {
    let TableFactor::Table { name, alias, .. } = factor else {
        return Err(ExecError::Message("JOIN requires base tables".into()));
    };
    let table_name = object_name_to_string(name);
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

fn nested_loop_inner_join(
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

    let mut out = Vec::new();
    for lr in left_rows {
        for rr in &right_rows {
            if lr.get(left_idx).map(|v| v.as_str()) == rr.get(right_idx).map(|v| v.as_str()) {
                let mut row = lr.clone();
                row.extend(rr.clone());
                out.push(row);
            }
        }
    }
    Ok((combined_columns, out))
}

fn execute_join_select<E: StorageEngine>(
    engine: &mut E,
    session: &Session,
    select: &sqlparser::ast::Select,
    from: &sqlparser::ast::TableWithJoins,
    order_by: Option<&OrderBy>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<QueryResult, ExecError> {
    if from.joins.len() != 1 {
        return Err(ExecError::Message(
            "only single INNER JOIN supported".into(),
        ));
    }
    let join = &from.joins[0];
    let JoinOperator::Inner(JoinConstraint::On(on_expr)) = &join.join_operator else {
        return Err(ExecError::Message(
            "only INNER JOIN ... ON supported".into(),
        ));
    };

    let left = join_side_from_factor(&from.relation, session)?;
    let right = join_side_from_factor(&join.relation, session)?;
    let key = parse_join_on_eq(on_expr)?;

    let left_rows = engine.scan(&left.table_name)?;
    let right_rows = engine.scan(&right.table_name)?;
    let (table_columns, mut rows) =
        nested_loop_inner_join(&left, left_rows, &right, right_rows, &key)?;

    if let Some(filter) = parse_where_filter(select.selection.as_ref())? {
        rows = filter_rows(rows, &table_columns, &filter)?;
    }

    let (out_columns, proj_indices) = resolve_projection(&select.projection, &table_columns)?;
    let (columns, rows) = finalize_select_rows(out_columns, proj_indices, table_columns, rows)?;
    let rows = finish_row_set(rows, &columns, order_by, offset, limit)?;
    Ok(QueryResult::Rows { columns, rows })
}

fn table_meta_from_create(create: &sqlparser::ast::CreateTable) -> TableMeta {
    let table_name = object_name_to_string(&create.name);
    let mut columns: Vec<ColumnDef> = create
        .columns
        .iter()
        .map(|c| {
            let mut col = ColumnDef::new(c.name.value.clone(), c.data_type.to_string());
            for opt in &c.options {
                match &opt.option {
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
        })
        .collect();

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
    }

    TableMeta {
        name: table_name,
        columns,
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

/// Stored representation of SQL NULL in heap rows (empty string).
fn sql_null() -> String {
    String::new()
}

fn is_null_cell(cell: &str) -> bool {
    cell.is_empty()
}

fn expr_to_string(expr: &Expr) -> Result<String, ExecError> {
    match expr {
        Expr::Value(Value::Null) => Ok(sql_null()),
        Expr::Value(Value::Number(n, _)) => Ok(n.clone()),
        Expr::Value(Value::SingleQuotedString(s)) => Ok(s.clone()),
        other => Err(ExecError::Message(format!("unsupported expr: {other:?}"))),
    }
}

fn extract_eq_predicate(selection: Option<&Expr>) -> Option<(String, String)> {
    let filter = parse_where_filter(selection).ok()??;
    let WhereFilter::Pred(Predicate::Compare(pred)) = filter else {
        return None;
    };
    if pred.op != CompareOp::Eq {
        return None;
    }
    Some((pred.column, pred.value))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompareOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Debug, Clone)]
struct LiteralPredicate {
    column: String,
    op: CompareOp,
    value: String,
}

#[derive(Debug, Clone)]
enum Predicate {
    Compare(LiteralPredicate),
    IsNull { column: String },
    IsNotNull { column: String },
}

#[derive(Debug, Clone)]
enum WhereFilter {
    Pred(Predicate),
    And(Vec<WhereFilter>),
}

fn parse_where_filter(selection: Option<&Expr>) -> Result<Option<WhereFilter>, ExecError> {
    let Some(expr) = selection else {
        return Ok(None);
    };
    if let Expr::BinaryOp {
        left,
        op: BinaryOperator::And,
        right,
    } = expr
    {
        let mut parts = Vec::new();
        collect_and_exprs(left, &mut parts);
        collect_and_exprs(right, &mut parts);
        let filters = parts
            .into_iter()
            .map(|e| Ok(WhereFilter::Pred(parse_predicate(e)?)))
            .collect::<Result<Vec<_>, ExecError>>()?;
        return Ok(Some(WhereFilter::And(filters)));
    }
    Ok(Some(WhereFilter::Pred(parse_predicate(expr)?)))
}

fn parse_predicate(expr: &Expr) -> Result<Predicate, ExecError> {
    match expr {
        Expr::IsNull(inner) => Ok(Predicate::IsNull {
            column: expr_column_name(inner)?,
        }),
        Expr::IsNotNull(inner) => Ok(Predicate::IsNotNull {
            column: expr_column_name(inner)?,
        }),
        _ => Ok(Predicate::Compare(parse_literal_predicate(expr)?)),
    }
}

fn collect_and_exprs<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
    if let Expr::BinaryOp {
        left,
        op: BinaryOperator::And,
        right,
    } = expr
    {
        collect_and_exprs(left, out);
        collect_and_exprs(right, out);
    } else {
        out.push(expr);
    }
}

fn parse_literal_predicate(expr: &Expr) -> Result<LiteralPredicate, ExecError> {
    let Expr::BinaryOp { left, op, right } = expr else {
        return Err(ExecError::Message(format!(
            "unsupported WHERE expression: {expr:?}"
        )));
    };
    let column = match left.as_ref() {
        Expr::Identifier(id) => id.value.clone(),
        Expr::CompoundIdentifier(parts) => parts
            .last()
            .map(|id| id.value.clone())
            .ok_or_else(|| ExecError::Message("empty compound identifier".into()))?,
        other => {
            return Err(ExecError::Message(format!(
                "unsupported WHERE column: {other:?}"
            )))
        }
    };
    let op = match op {
        BinaryOperator::Eq => CompareOp::Eq,
        BinaryOperator::NotEq => CompareOp::NotEq,
        BinaryOperator::Lt => CompareOp::Lt,
        BinaryOperator::LtEq => CompareOp::LtEq,
        BinaryOperator::Gt => CompareOp::Gt,
        BinaryOperator::GtEq => CompareOp::GtEq,
        other => {
            return Err(ExecError::Message(format!(
                "unsupported WHERE operator: {other:?}"
            )))
        }
    };
    let value = expr_to_string(right)?;
    Ok(LiteralPredicate { column, op, value })
}

fn compare_values(cell: &str, op: CompareOp, literal: &str) -> bool {
    if let (Ok(a), Ok(b)) = (cell.parse::<i64>(), literal.parse::<i64>()) {
        return match op {
            CompareOp::Eq => a == b,
            CompareOp::NotEq => a != b,
            CompareOp::Lt => a < b,
            CompareOp::LtEq => a <= b,
            CompareOp::Gt => a > b,
            CompareOp::GtEq => a >= b,
        };
    }
    match op {
        CompareOp::Eq => cell == literal,
        CompareOp::NotEq => cell != literal,
        CompareOp::Lt => cell < literal,
        CompareOp::LtEq => cell <= literal,
        CompareOp::Gt => cell > literal,
        CompareOp::GtEq => cell >= literal,
    }
}

fn row_matches_filter(row: &Row, columns: &[String], filter: &WhereFilter) -> bool {
    match filter {
        WhereFilter::Pred(Predicate::Compare(pred)) => {
            let col_idx = columns
                .iter()
                .position(|c| c.eq_ignore_ascii_case(&pred.column));
            col_idx
                .and_then(|i| row.get(i))
                .map(|cell| compare_values(cell, pred.op, &pred.value))
                .unwrap_or(false)
        }
        WhereFilter::Pred(Predicate::IsNull { column }) => {
            let col_idx = columns.iter().position(|c| c.eq_ignore_ascii_case(column));
            col_idx
                .and_then(|i| row.get(i))
                .map(|cell| is_null_cell(cell.as_str()))
                .unwrap_or(true)
        }
        WhereFilter::Pred(Predicate::IsNotNull { column }) => {
            let col_idx = columns.iter().position(|c| c.eq_ignore_ascii_case(column));
            col_idx
                .and_then(|i| row.get(i))
                .map(|cell| !is_null_cell(cell.as_str()))
                .unwrap_or(false)
        }
        WhereFilter::And(parts) => parts.iter().all(|f| row_matches_filter(row, columns, f)),
    }
}

fn filter_rows(
    rows: Vec<Row>,
    columns: &[String],
    filter: &WhereFilter,
) -> Result<Vec<Row>, ExecError> {
    Ok(rows
        .into_iter()
        .filter(|r| row_matches_filter(r, columns, filter))
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
        exec.execute(&mut session, &plans).unwrap();

        let describe = parse("DESCRIBE pk_t").unwrap();
        let plans = plan(&session, describe);
        let results = exec.execute(&mut session, &plans).unwrap();
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
        let results = exec.execute(&mut session, &plans).unwrap();
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
            exec.execute(&mut session, &plans).unwrap();
        }

        let plans = plan(&session, parse("SELECT * FROM t LIMIT 2").unwrap());
        let results = exec.execute(&mut session, &plans).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => assert_eq!(rows.len(), 2),
            _ => panic!("expected rows"),
        }

        let plans = plan(
            &session,
            parse("SELECT name FROM t WHERE id = 3 LIMIT 1").unwrap(),
        );
        let results = exec.execute(&mut session, &plans).unwrap();
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
        let results = exec.execute(&mut session, &plans).unwrap();
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
            exec.execute(&mut session, &plans).unwrap();
        }

        let plans = plan(&session, parse("SELECT * FROM t ORDER BY id").unwrap());
        let results = exec.execute(&mut session, &plans).unwrap();
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
        let results = exec.execute(&mut session, &plans).unwrap();
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
    fn select_column_aliases() {
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

        let plans = plan(&session, parse("SELECT id AS user_id FROM users").unwrap());
        let results = exec.execute(&mut session, &plans).unwrap();
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
        let results = exec.execute(&mut session, &plans).unwrap();
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
            exec.execute(&mut session, &plans).unwrap();
        }

        let plans = plan(
            &session,
            parse("SELECT * FROM scores WHERE id >= 2").unwrap(),
        );
        let results = exec.execute(&mut session, &plans).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => assert_eq!(rows.len(), 2),
            _ => panic!("expected rows"),
        }

        let plans = plan(
            &session,
            parse("SELECT name FROM scores WHERE id = 2 AND name = 'b'").unwrap(),
        );
        let results = exec.execute(&mut session, &plans).unwrap();
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
            exec.execute(&mut session, &plans).unwrap();
        }

        let plans = plan(
            &session,
            parse(
                "SELECT orders.id, order_items.sku FROM orders INNER JOIN order_items ON orders.id = order_items.order_id",
            )
            .unwrap(),
        );
        let results = exec.execute(&mut session, &plans).unwrap();
        match &results[0] {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns, &vec!["id".to_string(), "sku".to_string()]);
                assert_eq!(rows, &vec![vec!["1".to_string(), "x".to_string()]]);
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
            exec.execute(&mut session, &plans).unwrap();
        }

        let plans = plan(
            &session,
            parse("SELECT id FROM nullable_t WHERE note IS NULL").unwrap(),
        );
        let results = exec.execute(&mut session, &plans).unwrap();
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
        let results = exec.execute(&mut session, &plans).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows, &vec![vec!["2".to_string()]]);
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

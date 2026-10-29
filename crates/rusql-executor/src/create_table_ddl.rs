//! CREATE TABLE … LIKE / AS SELECT (M154).

use crate::info_schema;
use crate::{
    collect_insert_source_rows, object_name_to_string, resolve_object_storage_key,
    unique_constraint_names_from_create, validate_foreign_keys, ExecError,
};
use rusql_core::{table_storage_key, ColumnDef, PrivilegeStore, Session, TableMeta};
use rusql_storage::{Row, StorageEngine};
use sqlparser::ast::CreateTable;
use std::collections::HashSet;

pub fn execute_create_table<E: StorageEngine>(
    engine: &mut E,
    session: &mut Session,
    create: &CreateTable,
    privileges: &PrivilegeStore,
    table_meta_from_create: impl FnOnce(&CreateTable, &str) -> Result<TableMeta, ExecError>,
) -> Result<crate::QueryResult, ExecError> {
    if create.like.is_some() && create.query.is_some() {
        return Err(ExecError::Message(
            rusql_i18n::messages::sql_create_table_like_and_as_mutually_exclusive(),
        ));
    }

    let target_key = resolve_object_storage_key(session, &create.name)?;
    if create.if_not_exists && session.catalog.get_table(&target_key).is_some() {
        return Ok(crate::QueryResult::Ok { rows_affected: 0 });
    }

    let (meta, ctas_rows): (TableMeta, Option<Vec<Row>>) =
        if let Some(like_name) = create.like.as_ref() {
            let source_key = resolve_object_storage_key(session, like_name)?;
            if session.catalog.is_view(&source_key) {
                return Err(ExecError::Message(
                    rusql_i18n::messages::sql_truncate_not_base_table(&source_key),
                ));
            }
            let source = session
                .catalog
                .get_table(&source_key)
                .cloned()
                .ok_or_else(|| {
                    ExecError::Storage(rusql_storage::StorageError::table_not_found(&source_key))
                })?;
            (
                table_meta_from_like(&source, create, &session.database)?,
                None,
            )
        } else if let Some(query) = create.query.as_ref() {
            let rows = collect_insert_source_rows(engine, session, Some(query), privileges)?;
            let meta = if create.columns.is_empty() {
                ctas_meta_inferred(create, &session.database, &rows, query)?
            } else {
                table_meta_from_create(create, &session.database)?
            };
            (meta, Some(rows))
        } else {
            (table_meta_from_create(create, &session.database)?, None)
        };

    if rusql_sql::take_pending_range_partition().is_some() {
        return Err(ExecError::Message(
            rusql_i18n::messages::sql_range_partition_table_mismatch(),
        ));
    }

    validate_foreign_keys(session, &meta)?;
    let unique_names = if create.like.is_some() || create.query.is_some() {
        Vec::new()
    } else {
        unique_constraint_names_from_create(create)
    };

    engine.create_table(meta.clone())?;
    session.catalog.create_table(meta.clone());
    let table_key = table_storage_key(&meta.schema, &meta.name);
    info_schema::set_unique_constraints(&table_key, unique_names);
    for pending in rusql_sql::take_pending_functional_indexes() {
        crate::functional_index::create_pending_functional_index(engine, &table_key, &pending)?;
    }
    crate::fulltext::create_table_fulltext_indexes(
        engine,
        session,
        &table_key,
        &create.constraints,
    )?;

    if let Some(rows) = ctas_rows {
        for row in rows {
            engine.insert(&table_key, row)?;
        }
        return Ok(crate::QueryResult::Ok { rows_affected: 0 });
    }

    Ok(crate::QueryResult::Ok { rows_affected: 0 })
}

fn table_meta_from_like(
    source: &TableMeta,
    create: &CreateTable,
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

    let columns: Vec<ColumnDef> = source
        .columns
        .iter()
        .filter(|c| c.virtual_generated.is_some() || !is_stored_generated_skipped(c))
        .cloned()
        .collect();

    let auto_increment_next = if columns.iter().any(|c| c.auto_increment) {
        Some(1)
    } else {
        None
    };

    Ok(TableMeta {
        name: table_name,
        schema,
        columns,
        auto_increment_next,
        foreign_keys: Vec::new(),
        check_constraints: source.check_constraints.clone(),
        range_partition: None,
    })
}

fn is_stored_generated_skipped(_col: &ColumnDef) -> bool {
    false
}

fn ctas_meta_inferred(
    create: &CreateTable,
    default_schema: &str,
    rows: &[Row],
    query: &sqlparser::ast::Query,
) -> Result<TableMeta, ExecError> {
    let column_names = ctas_column_names(query)?;
    if column_names.is_empty() {
        return Err(ExecError::Message(
            rusql_i18n::messages::sql_create_table_as_select_no_columns(),
        ));
    }
    let parts: Vec<_> = create.name.0.iter().map(|i| i.value.clone()).collect();
    let (schema, table_name) = match parts.as_slice() {
        [t] => (default_schema.to_string(), t.clone()),
        [s, t] => (s.clone(), t.clone()),
        _ => (
            default_schema.to_string(),
            object_name_to_string(&create.name),
        ),
    };

    let mut columns = Vec::with_capacity(column_names.len());
    for (idx, name) in column_names.into_iter().enumerate() {
        let values: Vec<String> = rows.iter().filter_map(|r| r.get(idx).cloned()).collect();
        let data_type = infer_ctas_column_type(&values);
        columns.push(ColumnDef::new(name, data_type));
    }

    Ok(TableMeta {
        name: table_name,
        schema,
        columns,
        auto_increment_next: None,
        foreign_keys: Vec::new(),
        check_constraints: Vec::new(),
        range_partition: None,
    })
}

fn ctas_column_names(query: &sqlparser::ast::Query) -> Result<Vec<String>, ExecError> {
    use sqlparser::ast::{SelectItem, SetExpr};

    let select = match query.body.as_ref() {
        SetExpr::Select(s) => s,
        _ => {
            return Err(ExecError::Message(
                rusql_i18n::messages::sql_create_table_as_select_requires_select(),
            ))
        }
    };
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    for item in &select.projection {
        let label = match item {
            SelectItem::UnnamedExpr(expr) => super::expr::expr_output_name(expr, None)?,
            SelectItem::ExprWithAlias { alias, .. } => alias.value.clone(),
            SelectItem::QualifiedWildcard(_, _) | SelectItem::Wildcard(_) => {
                return Err(ExecError::Message(
                    rusql_i18n::messages::sql_create_table_as_select_no_wildcard(),
                ))
            }
        };
        let unique = dedupe_ctas_column_name(&label, &mut seen);
        names.push(unique);
    }
    Ok(names)
}

fn dedupe_ctas_column_name(base: &str, seen: &mut HashSet<String>) -> String {
    if seen.insert(base.to_string()) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}_{n}");
        if seen.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

fn infer_ctas_column_type(values: &[String]) -> String {
    let non_empty: Vec<&str> = values
        .iter()
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .collect();
    if non_empty.is_empty() {
        return "VARCHAR(255)".to_string();
    }
    if non_empty.iter().all(|v| v.parse::<i64>().is_ok()) {
        return "INT".to_string();
    }
    if non_empty
        .iter()
        .all(|v| v.parse::<f64>().is_ok() && !v.contains('e') && !v.contains('E'))
    {
        if non_empty.iter().any(|v| v.contains('.')) {
            return "DOUBLE".to_string();
        }
        return "INT".to_string();
    }
    let max_len = non_empty.iter().map(|s| s.len()).max().unwrap_or(1);
    format!("VARCHAR({})", max_len.max(1))
}

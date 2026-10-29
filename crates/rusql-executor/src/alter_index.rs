//! ALTER TABLE index DDL and unique-index enforcement (M153).

use rusql_core::{IndexMeta, TableMeta};
use rusql_storage::StorageEngine;
use sqlparser::ast::{Expr, Ident, OrderByExpr, TableConstraint};

use crate::functional_index;
use crate::info_schema;
use crate::ExecError;

fn order_exprs_from_idents(idents: &[Ident]) -> Vec<OrderByExpr> {
    idents
        .iter()
        .map(|id| OrderByExpr {
            expr: Expr::Identifier(id.clone()),
            asc: None,
            nulls_first: None,
            with_fill: None,
        })
        .collect()
}

pub fn index_name_from_constraint(
    table: &str,
    name: &Option<Ident>,
    index_name: &Option<Ident>,
    columns: &[OrderByExpr],
    default_prefix: &str,
) -> String {
    name.as_ref()
        .or(index_name.as_ref())
        .map(|i| i.value.clone())
        .unwrap_or_else(|| {
            columns
                .first()
                .and_then(|c| match &c.expr {
                    Expr::Identifier(id) => Some(id.value.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| format!("{default_prefix}_{table}"))
        })
}

pub fn add_index_from_constraint<E: StorageEngine>(
    engine: &mut E,
    session: &rusql_core::Session,
    table_key: &str,
    constraint: &TableConstraint,
) -> Result<(), ExecError> {
    match constraint {
        TableConstraint::Index { name, columns, .. } => {
            let order_cols = order_exprs_from_idents(columns);
            let index_name = index_name_from_constraint(table_key, &None, name, &order_cols, "idx");
            let meta = functional_index::index_meta_from_order_columns(
                table_key,
                &index_name,
                &order_cols,
            )?;
            engine.create_index(meta)?;
            Ok(())
        }
        TableConstraint::Unique {
            name,
            index_name,
            columns,
            ..
        } => {
            let order_cols = order_exprs_from_idents(columns);
            let idx = index_name_from_constraint(table_key, name, index_name, &order_cols, "uniq");
            let meta =
                functional_index::index_meta_from_order_columns(table_key, &idx, &order_cols)?;
            engine.create_index(meta.clone())?;
            if let Err(e) = engine.validate_unique_index(table_key, &idx) {
                let _ = engine.drop_index(table_key, &idx, true);
                return Err(e.into());
            }
            info_schema::add_unique_constraint(table_key, &idx);
            Ok(())
        }
        TableConstraint::FulltextOrSpatial {
            fulltext: false, ..
        } => Err(ExecError::Mysql {
            code: crate::fulltext::ER_NOT_SUPPORTED_YET,
            message: rusql_i18n::messages::sql_ft_not_supported("SPATIAL INDEX"),
        }),
        TableConstraint::FulltextOrSpatial {
            fulltext: true,
            opt_index_name,
            columns,
            ..
        } => crate::fulltext::create_fulltext_on_columns(
            engine,
            session,
            table_key,
            opt_index_name.as_ref(),
            columns,
        ),
        other => Err(ExecError::Message(format!(
            "unsupported ADD constraint for index DDL: {other}"
        ))),
    }
}

pub fn drop_index_by_name<E: StorageEngine>(
    engine: &mut E,
    session: &mut rusql_core::Session,
    table_key: &str,
    index_name: &str,
    if_exists: bool,
) -> Result<(), ExecError> {
    engine.drop_index(table_key, index_name, if_exists)?;
    info_schema::remove_unique_constraint(table_key, index_name);
    if index_name.eq_ignore_ascii_case("PRIMARY") {
        catalog_clear_primary_key(session, table_key);
    }
    Ok(())
}

pub fn drop_primary_key<E: StorageEngine>(
    engine: &mut E,
    session: &mut rusql_core::Session,
    table_key: &str,
) -> Result<(), ExecError> {
    drop_index_by_name(engine, session, table_key, "PRIMARY", false)?;
    Ok(())
}

fn catalog_clear_primary_key(session: &mut rusql_core::Session, table_key: &str) {
    let Some(mut meta) = session.catalog.get_table(table_key).cloned() else {
        return;
    };
    for col in &mut meta.columns {
        col.primary_key = false;
    }
    session.catalog.create_table(meta);
}

pub fn check_unique_indexes<E: StorageEngine>(
    engine: &E,
    session: &rusql_core::Session,
    table_key: &str,
    meta: &TableMeta,
    row: &rusql_storage::Row,
    skip_row: Option<&rusql_storage::Row>,
) -> Result<(), ExecError> {
    let names = info_schema::unique_constraint_names(table_key);
    if names.is_empty() {
        return Ok(());
    }
    let metas = engine.index_metas();
    for name in names {
        let Some(def) = metas
            .iter()
            .find(|d| d.table == table_key && d.name.eq_ignore_ascii_case(&name))
        else {
            continue;
        };
        if def.is_functional() {
            continue;
        }
        let eq = index_eq_from_row(meta, def, row)?;
        if eq.is_empty() {
            continue;
        }
        if eq.iter().any(|(_, v)| v.is_empty()) {
            continue;
        }
        let refs: Vec<(&str, &str)> = eq.iter().map(|(c, v)| (c.as_str(), v.as_str())).collect();
        let Some(rows) = engine.scan_eq_prefix(table_key, &refs)? else {
            continue;
        };
        for existing in rows {
            if skip_row.is_some_and(|s| s == &existing) {
                continue;
            }
            let value = eq
                .iter()
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>()
                .join("-");
            return Err(ExecError::Mysql {
                code: 1062,
                message: rusql_i18n::messages::sql_duplicate_entry(&value, &name),
            });
        }
    }
    let _ = session;
    Ok(())
}

fn index_eq_from_row(
    meta: &TableMeta,
    def: &IndexMeta,
    row: &rusql_storage::Row,
) -> Result<Vec<(String, String)>, ExecError> {
    let mut out = Vec::with_capacity(def.columns.len());
    for col in &def.columns {
        let idx = meta
            .columns
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(col))
            .ok_or_else(|| ExecError::Message(format!("column '{col}' not found")))?;
        out.push((col.clone(), row.get(idx).cloned().unwrap_or_default()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use crate::{heap_executor, ExecError, QueryResult};
    use rusql_core::Session;
    use rusql_planner::plan;
    use rusql_sql::parse;

    #[test]
    fn alter_index_add_drop_and_unique_enforced() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE ai_t (id INT, n INT)",
            "INSERT INTO ai_t VALUES (1, 10), (2, 20)",
            "ALTER TABLE ai_t ADD INDEX idx_n (n)",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }
        let plans = plan(&session, parse("SELECT id FROM ai_t WHERE n = 20").unwrap());
        let results = exec.execute(&mut session, &plans, None).unwrap();
        match &results[0] {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows, &vec![vec!["2".to_string()]]);
            }
            other => panic!("expected index lookup row, got {other:?}"),
        }

        let plans = plan(
            &session,
            parse("ALTER TABLE ai_t DROP INDEX idx_n").unwrap(),
        );
        exec.execute(&mut session, &plans, None).unwrap();

        let plans = plan(&session, parse("ALTER TABLE ai_t ADD UNIQUE (n)").unwrap());
        exec.execute(&mut session, &plans, None).unwrap();

        let plans = plan(&session, parse("INSERT INTO ai_t VALUES (3, 10)").unwrap());
        let err = exec.execute(&mut session, &plans, None).unwrap_err();
        match err {
            ExecError::Mysql { code: 1062, .. } => {}
            other => panic!("expected duplicate unique 1062, got {other:?}"),
        }
    }

    #[test]
    fn alter_index_drop_index_on_table_form() {
        let mut session = Session::new(1, "root");
        let mut exec = heap_executor();
        for sql in [
            "CREATE TABLE ai_t2 (id INT, n INT)",
            "ALTER TABLE ai_t2 ADD INDEX idx_n (n)",
            "DROP INDEX idx_n ON ai_t2",
        ] {
            let plans = plan(&session, parse(sql).unwrap());
            exec.execute(&mut session, &plans, None).unwrap();
        }
        let plans = plan(
            &session,
            parse("ALTER TABLE ai_t2 ADD INDEX idx_n (n)").unwrap(),
        );
        exec.execute(&mut session, &plans, None).unwrap();
    }
}

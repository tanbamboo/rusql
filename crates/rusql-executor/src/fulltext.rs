//! FULLTEXT index DDL and `MATCH … AGAINST` execution (M197).

use rusql_core::{type_base, IndexMeta, Session};
use rusql_storage::{fulltext_columns_match, FulltextBooleanQuery, Row, StorageEngine};
use sqlparser::ast::{Ident, TableConstraint};

use crate::where_filter::{FulltextQuery, Predicate, WhereFilter};
use crate::ExecError;

/// MySQL `ER_FT_MATCHING_KEY_NOT_FOUND`.
pub const ER_FT_MATCHING_KEY_NOT_FOUND: u16 = 1191;
/// MySQL `ER_BAD_FT_COLUMN`.
pub const ER_BAD_FT_COLUMN: u16 = 1283;
/// MySQL `ER_KEY_COLUMN_DOES_NOT_EXITS`.
pub const ER_KEY_COLUMN_DOES_NOT_EXIST: u16 = 1072;
/// MySQL `ER_NOT_SUPPORTED_YET`.
pub const ER_NOT_SUPPORTED_YET: u16 = 1235;

const FT_NATURAL: &str = "MATCH ... IN NATURAL LANGUAGE MODE";
const FT_EXPANSION: &str = "MATCH ... WITH QUERY EXPANSION";
const FT_BOOLEAN_OP: &str = "MATCH ... IN BOOLEAN MODE operator";
const FT_SPATIAL: &str = "SPATIAL INDEX";

fn ft_types_ok(data_type: &str) -> bool {
    matches!(
        type_base(data_type).as_str(),
        "CHAR" | "VARCHAR" | "TINYTEXT" | "TEXT" | "MEDIUMTEXT" | "LONGTEXT"
    )
}

pub fn missing_index() -> ExecError {
    ExecError::Mysql {
        code: ER_FT_MATCHING_KEY_NOT_FOUND,
        message: rusql_i18n::messages::sql_ft_matching_key_not_found(),
    }
}

fn not_supported(feature: &str) -> ExecError {
    ExecError::Mysql {
        code: ER_NOT_SUPPORTED_YET,
        message: rusql_i18n::messages::sql_ft_not_supported(feature),
    }
}

/// Mark `meta` as FULLTEXT and reject non-text or missing columns.
pub fn enforce_fulltext_meta(
    session: &Session,
    table: &str,
    meta: &mut IndexMeta,
) -> Result<(), ExecError> {
    if meta.functional_key.is_some() || meta.columns.is_empty() {
        return Err(not_supported(FT_BOOLEAN_OP));
    }
    let table_meta = session
        .catalog
        .get_table(table)
        .ok_or_else(|| ExecError::Storage(rusql_storage::StorageError::table_not_found(table)))?;
    if table_meta.range_partition.is_some() {
        return Err(not_supported("FULLTEXT on a partitioned table"));
    }
    for column in &meta.columns {
        let Some(def) = table_meta
            .columns
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(column))
        else {
            return Err(ExecError::Mysql {
                code: ER_KEY_COLUMN_DOES_NOT_EXIST,
                message: rusql_i18n::messages::sql_ft_key_column_missing(column),
            });
        };
        if !ft_types_ok(&def.data_type) {
            return Err(ExecError::Mysql {
                code: ER_BAD_FT_COLUMN,
                message: rusql_i18n::messages::sql_ft_bad_column(column),
            });
        }
    }
    meta.fulltext = true;
    Ok(())
}

pub fn create_table_fulltext_indexes<E: StorageEngine>(
    engine: &mut E,
    session: &Session,
    table_key: &str,
    constraints: &[TableConstraint],
) -> Result<(), ExecError> {
    for constraint in constraints {
        if let TableConstraint::FulltextOrSpatial {
            fulltext,
            opt_index_name,
            columns,
            ..
        } = constraint
        {
            if !*fulltext {
                return Err(not_supported(FT_SPATIAL));
            }
            create_fulltext_on_columns(
                engine,
                session,
                table_key,
                opt_index_name.as_ref(),
                columns,
            )?;
        }
    }
    Ok(())
}

pub fn create_fulltext_on_columns<E: StorageEngine>(
    engine: &mut E,
    session: &Session,
    table_key: &str,
    index_name: Option<&Ident>,
    columns: &[Ident],
) -> Result<(), ExecError> {
    if columns.is_empty() {
        return Err(not_supported(FT_BOOLEAN_OP));
    }
    let name = index_name
        .map(|ident| ident.value.clone())
        .unwrap_or_else(|| columns[0].value.clone());
    let cols = columns.iter().map(|ident| ident.value.clone()).collect();
    let mut meta = IndexMeta::new(name, table_key, cols);
    enforce_fulltext_meta(session, table_key, &mut meta)?;
    engine.create_index(meta).map_err(ExecError::from)
}

fn index_covers<E: StorageEngine>(engine: &E, table: Option<&str>, columns: &[String]) -> bool {
    engine.index_metas().iter().any(|meta| {
        meta.fulltext
            && fulltext_columns_match(&meta.columns, columns)
            && match table {
                None => true,
                Some(name) => meta.table == name,
            }
    })
}

fn walk_fulltext<'a>(filter: &'a WhereFilter, out: &mut Vec<&'a Predicate>) {
    match filter {
        WhereFilter::Pred(pred @ Predicate::Fulltext { .. }) => out.push(pred),
        WhereFilter::And(parts) | WhereFilter::Or(parts) => {
            for part in parts {
                walk_fulltext(part, out);
            }
        }
        WhereFilter::Not(inner) => walk_fulltext(inner, out),
        WhereFilter::Pred(_) => {}
    }
}

/// Errno **1191** when a `MATCH` column list has no FULLTEXT index.
/// Errno **1235** when the index exists but the search modifier is outside this slice.
pub fn guard_fulltext<E: StorageEngine>(
    engine: &E,
    table: Option<&str>,
    filter: &WhereFilter,
) -> Result<(), ExecError> {
    let mut preds = Vec::new();
    walk_fulltext(filter, &mut preds);
    for pred in preds {
        let Predicate::Fulltext { columns, query } = pred else {
            continue;
        };
        if !index_covers(engine, table, columns) {
            return Err(missing_index());
        }
        if let FulltextQuery::Rejected(feature) = query {
            return Err(not_supported(feature));
        }
    }
    Ok(())
}

/// `Some(rows)` when `filter` is only `MATCH … AGAINST` and the index can answer it.
pub fn try_fulltext_scan<E: StorageEngine>(
    engine: &E,
    table: &str,
    filter: &WhereFilter,
) -> Result<Option<Vec<Row>>, ExecError> {
    let WhereFilter::Pred(Predicate::Fulltext { columns, query }) = filter else {
        return Ok(None);
    };
    if !index_covers(engine, Some(table), columns) {
        return Err(missing_index());
    }
    let query = match query {
        FulltextQuery::Boolean(query) => query,
        FulltextQuery::Rejected(feature) => return Err(not_supported(feature)),
    };
    match engine.fulltext_search(table, columns, query)? {
        Some(rows) => Ok(Some(rows)),
        None => Err(missing_index()),
    }
}

pub fn natural_language_feature() -> &'static str {
    FT_NATURAL
}

pub fn query_expansion_feature() -> &'static str {
    FT_EXPANSION
}

pub fn boolean_operator_feature() -> &'static str {
    FT_BOOLEAN_OP
}

/// Concatenate MATCH columns for the same tokenizer the index uses.
pub fn match_document(row: &Row, table_columns: &[String], match_columns: &[String]) -> String {
    let mut text = String::new();
    for column in match_columns {
        let Some(idx) = table_columns
            .iter()
            .position(|name| name.eq_ignore_ascii_case(column))
        else {
            continue;
        };
        if let Some(cell) = row.get(idx) {
            text.push(' ');
            text.push_str(cell);
        }
    }
    text
}

pub fn boolean_query_matches(text: &str, query: &FulltextBooleanQuery) -> bool {
    rusql_storage::document_matches(text, query)
}

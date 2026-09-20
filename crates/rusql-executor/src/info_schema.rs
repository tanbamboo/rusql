//! Virtual information_schema and DESCRIBE result helpers.

use rusql_core::{
    column_type_display, data_type_name, table_storage_key, Collation, EventMeta, Session,
    TableMeta, ViewMeta, DEFAULT_COLLATION as CORE_DEFAULT_COLLATION,
};
use rusql_storage::{Row, StorageEngine};

use crate::{ExecError, QueryResult};

pub const DEFAULT_SCHEMA: &str = "rusql";
pub const DEFAULT_CHARSET: &str = "utf8mb4";
pub const DEFAULT_COLLATION: &str = "utf8mb4_unicode_ci";

const SHOW_COLLATION_COLUMNS: [&str; 7] = [
    "Collation",
    "Charset",
    "Id",
    "Default",
    "Compiled",
    "Sortlen",
    "Pad_attribute",
];

const DESCRIBE_COLUMNS: [&str; 6] = ["Field", "Type", "Null", "Key", "Default", "Extra"];

const INFO_TABLES_COLUMNS: [&str; 3] = ["TABLE_SCHEMA", "TABLE_NAME", "TABLE_TYPE"];

const INFO_COLUMNS_COLUMNS: [&str; 8] = [
    "TABLE_SCHEMA",
    "TABLE_NAME",
    "COLUMN_NAME",
    "ORDINAL_POSITION",
    "DATA_TYPE",
    "COLUMN_TYPE",
    "IS_NULLABLE",
    "COLUMN_COLLATION",
];

const INFO_SCHEMATA_COLUMNS: [&str; 3] = [
    "SCHEMA_NAME",
    "DEFAULT_CHARACTER_SET_NAME",
    "DEFAULT_COLLATION_NAME",
];

const INFO_STATISTICS_COLUMNS: [&str; 7] = [
    "TABLE_SCHEMA",
    "TABLE_NAME",
    "INDEX_NAME",
    "SEQ_IN_INDEX",
    "COLUMN_NAME",
    "NON_UNIQUE",
    "INDEX_TYPE",
];

const INFO_KEY_COLUMN_USAGE_COLUMNS: [&str; 7] = [
    "TABLE_SCHEMA",
    "TABLE_NAME",
    "COLUMN_NAME",
    "CONSTRAINT_NAME",
    "REFERENCED_TABLE_SCHEMA",
    "REFERENCED_TABLE_NAME",
    "REFERENCED_COLUMN_NAME",
];

const INFO_ROUTINES_COLUMNS: [&str; 4] = [
    "ROUTINE_SCHEMA",
    "ROUTINE_NAME",
    "ROUTINE_TYPE",
    "DTD_IDENTIFIER",
];

const INFO_TRIGGERS_COLUMNS: [&str; 6] = [
    "TRIGGER_SCHEMA",
    "TRIGGER_NAME",
    "EVENT_MANIPULATION",
    "EVENT_OBJECT_TABLE",
    "ACTION_TIMING",
    "ACTION_STATEMENT",
];

/// Documented MySQL-like `information_schema.EVENTS` columns (M109).
/// No CREATED / LAST_ALTERED stubs — those would invent timestamps.
const INFO_EVENTS_COLUMNS: [&str; 13] = [
    "EVENT_SCHEMA",
    "EVENT_NAME",
    "DEFINER",
    "EVENT_TYPE",
    "EXECUTE_AT",
    "INTERVAL_VALUE",
    "INTERVAL_FIELD",
    "STARTS",
    "ENDS",
    "STATUS",
    "ON_COMPLETION",
    "LAST_EXECUTED",
    "EVENT_COMMENT",
];

const EVENTS_STUB_DEFINER: &str = "root@%";
const EVENTS_DEFAULT_ON_COMPLETION: &str = "NOT PRESERVE";

const SHOW_INDEX_COLUMNS: [&str; 6] = [
    "Table",
    "Non_unique",
    "Key_name",
    "Seq_in_index",
    "Column_name",
    "Index_type",
];

pub const SHOW_INDEX_VIRTUAL_TABLE: &str = "__rusql_show_index";

const PROCESSLIST_COLUMNS: [&str; 8] = [
    "Id", "User", "Host", "db", "Command", "Time", "State", "Info",
];

pub const PROCESSLIST_VIRTUAL_TABLE: &str = "__rusql_processlist";

/// DESCRIBE / SHOW COLUMNS result for one table.
pub fn describe_table(meta: &TableMeta) -> QueryResult {
    let rows: Vec<Row> = meta
        .columns
        .iter()
        .map(|c| {
            vec![
                c.name.clone(),
                column_type_display(&c.data_type),
                if c.nullable {
                    "YES".into()
                } else {
                    "NO".into()
                },
                if c.primary_key {
                    "PRI".into()
                } else {
                    "".into()
                },
                "NULL".into(),
                if c.auto_increment {
                    "auto_increment".into()
                } else {
                    "".into()
                },
            ]
        })
        .collect();
    QueryResult::Rows {
        columns: DESCRIBE_COLUMNS.iter().map(|s| (*s).to_string()).collect(),
        rows,
    }
}

pub fn describe_table_by_name(session: &Session, table: &str) -> Result<QueryResult, ExecError> {
    let meta =
        session.catalog.get_table(table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(table))
        })?;
    Ok(describe_table(&meta))
}

/// `SHOW CREATE TABLE` result (Table, Create Table).
pub fn show_create_table(meta: &TableMeta) -> QueryResult {
    let col_defs: Vec<String> = meta
        .columns
        .iter()
        .map(|c| {
            let mut def = format!("`{}` {}", c.name, c.data_type.to_uppercase());
            if !c.nullable {
                def.push_str(" NOT NULL");
            }
            if c.auto_increment {
                def.push_str(" AUTO_INCREMENT");
            }
            if c.primary_key {
                def.push_str(" PRIMARY KEY");
            }
            def
        })
        .collect();
    let mut ddl = format!("CREATE TABLE `{}` ({})", meta.name, col_defs.join(", "));
    if let Some(n) = meta.auto_increment_next {
        ddl.push_str(&format!(" AUTO_INCREMENT={n}"));
    }
    QueryResult::Rows {
        columns: vec!["Table".into(), "Create Table".into()],
        rows: vec![vec![meta.name.clone(), ddl]],
    }
}

pub fn show_create_table_by_name(session: &Session, table: &str) -> Result<QueryResult, ExecError> {
    let meta =
        session.catalog.get_table(table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(table))
        })?;
    Ok(show_create_table(&meta))
}

/// `SHOW CREATE VIEW` result (View, Create View, character_set_client, collation_connection).
/// DDL is reconstructed from the catalog SELECT — not ALGORITHM / DEFINER / SQL SECURITY.
pub fn show_create_view(meta: &ViewMeta) -> QueryResult {
    let display = view_display_name(&meta.name);
    let ident = display.replace('`', "``");
    let ddl = format!("CREATE VIEW `{ident}` AS {}", meta.sql);
    QueryResult::Rows {
        columns: vec![
            "View".into(),
            "Create View".into(),
            "character_set_client".into(),
            "collation_connection".into(),
        ],
        rows: vec![vec![
            display.to_string(),
            ddl,
            DEFAULT_CHARSET.to_string(),
            DEFAULT_COLLATION.to_string(),
        ]],
    }
}

pub fn show_create_view_by_name(session: &Session, view: &str) -> Result<QueryResult, ExecError> {
    let meta =
        session.catalog.get_view(view).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(view))
        })?;
    Ok(show_create_view(&meta))
}

fn view_display_name(storage_key: &str) -> &str {
    storage_key.rsplit('.').next().unwrap_or(storage_key)
}

/// `SELECT * FROM information_schema.tables`
pub fn scan_information_schema_tables<E: StorageEngine>(
    engine: &E,
    session: &Session,
    schema: &str,
) -> QueryResult {
    let mut names: std::collections::HashSet<String> =
        engine.table_names_in(schema).into_iter().collect();
    for view in session.catalog.view_names() {
        let bare = if schema == DEFAULT_SCHEMA {
            if view.contains('.') {
                continue;
            }
            view.clone()
        } else if let Some(rest) = view.strip_prefix(&format!("{schema}.")) {
            rest.to_string()
        } else {
            continue;
        };
        names.insert(bare);
    }
    let mut names: Vec<_> = names.into_iter().collect();
    names.sort();
    let rows: Vec<Row> = names
        .into_iter()
        .map(|t| {
            let key = table_storage_key(schema, &t);
            let kind = if session.catalog.is_view(&key) {
                "VIEW"
            } else {
                "BASE TABLE"
            };
            vec![schema.into(), t, kind.into()]
        })
        .collect();
    QueryResult::Rows {
        columns: INFO_TABLES_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    }
}

const INFO_VIEWS_COLUMNS: [&str; 3] = ["TABLE_SCHEMA", "TABLE_NAME", "VIEW_DEFINITION"];

/// `SELECT * FROM information_schema.VIEWS`
pub fn scan_information_schema_views(session: &Session) -> QueryResult {
    let mut names: Vec<_> = session.catalog.view_names().cloned().collect();
    names.sort();
    let rows: Vec<Row> = names
        .into_iter()
        .filter_map(|name| {
            session.catalog.get_view(&name).map(|view| {
                vec![
                    session.database.clone(),
                    view.name.clone(),
                    view.sql.clone(),
                ]
            })
        })
        .collect();
    QueryResult::Rows {
        columns: INFO_VIEWS_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    }
}

/// `SELECT * FROM information_schema.columns [WHERE table_name = '…']`
pub fn scan_information_schema_columns<E: StorageEngine>(
    engine: &E,
    session: &Session,
    table_filter: Option<&str>,
) -> Result<QueryResult, ExecError> {
    let mut names = engine.table_names_in(&session.database);
    names.sort();
    let mut rows = Vec::new();
    for table in names {
        if table_filter.is_some_and(|f| !f.eq_ignore_ascii_case(&table)) {
            continue;
        }
        let key = table_storage_key(&session.database, &table);
        let meta = session.catalog.get_table(&key).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(&table))
        })?;
        for (i, col) in meta.columns.iter().enumerate() {
            rows.push(vec![
                session.database.clone(),
                table.clone(),
                col.name.clone(),
                (i + 1).to_string(),
                data_type_name(&col.data_type),
                column_type_display(&col.data_type),
                if col.nullable {
                    "YES".into()
                } else {
                    "NO".into()
                },
                DEFAULT_COLLATION.into(),
            ]);
        }
    }
    Ok(QueryResult::Rows {
        columns: INFO_COLUMNS_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    })
}

/// `SELECT * FROM information_schema.SCHEMATA`
pub fn scan_information_schema_schemata(schemas: &[String]) -> QueryResult {
    let rows: Vec<Row> = schemas
        .iter()
        .map(|schema| {
            vec![
                schema.clone(),
                DEFAULT_CHARSET.into(),
                DEFAULT_COLLATION.into(),
            ]
        })
        .collect();
    QueryResult::Rows {
        columns: INFO_SCHEMATA_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    }
}

/// `SHOW INDEX FROM tbl` — MySQL-style index listing for one table.
pub fn show_index_for_table<E: StorageEngine>(
    engine: &E,
    session: &Session,
    table: &str,
) -> Result<QueryResult, ExecError> {
    let meta =
        session.catalog.get_table(table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(table))
        })?;
    let mut rows = Vec::new();
    let index_metas: Vec<_> = engine
        .index_metas()
        .into_iter()
        .filter(|idx| idx.table.eq_ignore_ascii_case(table))
        .collect();
    let has_primary_idx = index_metas.iter().any(|idx| idx.name == "PRIMARY");
    if !has_primary_idx {
        for col in &meta.columns {
            if col.primary_key {
                rows.push(vec![
                    table.into(),
                    "0".into(),
                    "PRIMARY".into(),
                    "1".into(),
                    col.name.clone(),
                    "BTREE".into(),
                ]);
            }
        }
    }
    for idx in index_metas {
        for (seq, col) in idx.columns.iter().enumerate() {
            rows.push(vec![
                table.into(),
                if idx.name == "PRIMARY" {
                    "0".into()
                } else {
                    "1".into()
                },
                idx.name.clone(),
                (seq + 1).to_string(),
                col.clone(),
                "BTREE".into(),
            ]);
        }
    }
    rows.sort_by(|a, b| {
        (a[2].as_str(), a[4].as_str(), a[3].as_str()).cmp(&(
            b[2].as_str(),
            b[4].as_str(),
            b[3].as_str(),
        ))
    });
    Ok(QueryResult::Rows {
        columns: SHOW_INDEX_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    })
}

/// `SHOW PROCESSLIST` via internal virtual table.
pub fn show_processlist(session: &Session) -> Result<QueryResult, ExecError> {
    let registry = session
        .process_list
        .as_ref()
        .ok_or_else(|| ExecError::Message("process list unavailable".into()))?;
    let rows: Vec<Row> = registry
        .snapshot()
        .into_iter()
        .map(|r| {
            vec![
                r.id.to_string(),
                r.user,
                r.host,
                r.db,
                r.command,
                r.time.to_string(),
                r.state,
                r.info.unwrap_or_default(),
            ]
        })
        .collect();
    Ok(QueryResult::Rows {
        columns: PROCESSLIST_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    })
}

/// `SHOW COLLATION` — supported utf8mb4 collations (M59).
pub fn show_collation(filter: Option<&str>) -> QueryResult {
    let mut rows: Vec<Row> = Collation::supported()
        .iter()
        .enumerate()
        .map(|(i, c)| {
            vec![
                c.name().into(),
                c.charset().into(),
                (i as u32 + 1).to_string(),
                if *c == CORE_DEFAULT_COLLATION {
                    "Yes".into()
                } else {
                    "".into()
                },
                "Yes".into(),
                "8".into(),
                "PAD SPACE".into(),
            ]
        })
        .collect();
    if let Some(name) = filter {
        rows.retain(|r| r[0].eq_ignore_ascii_case(name));
    }
    QueryResult::Rows {
        columns: SHOW_COLLATION_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    }
}

/// `SELECT * FROM information_schema.STATISTICS`
pub fn scan_information_schema_statistics<E: StorageEngine>(
    engine: &E,
    session: &Session,
) -> Result<QueryResult, ExecError> {
    let schema = session.database.clone();
    let mut rows = Vec::new();
    let mut tables = engine.table_names();
    tables.sort();
    for table in tables {
        let meta = session.catalog.get_table(&table).cloned().ok_or_else(|| {
            ExecError::Storage(rusql_storage::StorageError::table_not_found(&table))
        })?;
        let display_name = meta.name.clone();
        let table_indexes: Vec<_> = engine
            .index_metas()
            .into_iter()
            .filter(|idx| idx.table.eq_ignore_ascii_case(&table))
            .collect();
        let has_primary_idx = table_indexes.iter().any(|idx| idx.name == "PRIMARY");
        if !has_primary_idx {
            for col in &meta.columns {
                if col.primary_key {
                    rows.push(vec![
                        schema.clone(),
                        display_name.clone(),
                        "PRIMARY".into(),
                        "1".into(),
                        col.name.clone(),
                        "0".into(),
                        "BTREE".into(),
                    ]);
                }
            }
        }
        for idx in table_indexes {
            for (seq, col) in idx.columns.iter().enumerate() {
                rows.push(vec![
                    schema.clone(),
                    display_name.clone(),
                    idx.name.clone(),
                    (seq + 1).to_string(),
                    col.clone(),
                    if idx.name == "PRIMARY" {
                        "0".into()
                    } else {
                        "1".into()
                    },
                    "BTREE".into(),
                ]);
            }
        }
    }
    rows.sort_by(|a, b| {
        (a[1].as_str(), a[2].as_str(), a[4].as_str(), a[3].as_str()).cmp(&(
            b[1].as_str(),
            b[2].as_str(),
            b[4].as_str(),
            b[3].as_str(),
        ))
    });
    Ok(QueryResult::Rows {
        columns: INFO_STATISTICS_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    })
}

/// `SELECT * FROM information_schema.KEY_COLUMN_USAGE`
pub fn scan_information_schema_key_column_usage(session: &Session) -> QueryResult {
    let mut rows: Vec<Row> = Vec::new();
    for meta in session.catalog.iter_tables() {
        for (i, fk) in meta.foreign_keys.iter().enumerate() {
            for (col, ref_col) in fk.columns.iter().zip(&fk.referenced_columns) {
                rows.push(vec![
                    meta.schema.clone(),
                    meta.name.clone(),
                    col.clone(),
                    fk.constraint_name(&meta.name, i),
                    fk.referenced_schema.clone(),
                    fk.referenced_table.clone(),
                    ref_col.clone(),
                ]);
            }
        }
    }
    rows.sort_by(|a, b| {
        a[0].cmp(&b[0])
            .then(a[1].cmp(&b[1]))
            .then(a[3].cmp(&b[3]))
            .then(a[2].cmp(&b[2]))
    });
    QueryResult::Rows {
        columns: INFO_KEY_COLUMN_USAGE_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    }
}

/// Stub `information_schema.ROUTINES` rows.
pub fn scan_information_schema_routines(session: &Session) -> QueryResult {
    let mut rows: Vec<Row> = session
        .catalog
        .iter_procedures()
        .map(|p| {
            vec![
                p.schema.clone(),
                p.name.clone(),
                "PROCEDURE".into(),
                "NULL".into(),
            ]
        })
        .collect();
    rows.extend(session.catalog.iter_functions().map(|f| {
        vec![
            f.schema.clone(),
            f.name.clone(),
            "FUNCTION".into(),
            f.return_type.clone(),
        ]
    }));
    QueryResult::Rows {
        columns: INFO_ROUTINES_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    }
}

/// Catalog `information_schema.EVENTS` rows (M109).
///
/// `DEFINER` matches `SHOW EVENTS`: empty catalog definer is stub `root@%`.
/// `ON_COMPLETION` defaults to `NOT PRESERVE` when unset.
/// `LAST_EXECUTED` / `EVENT_COMMENT` are empty strings when unset (no invented timestamps).
pub fn scan_information_schema_events(session: &Session) -> QueryResult {
    let mut events: Vec<&EventMeta> = session.catalog.iter_events().collect();
    events.sort_by(|a, b| a.schema.cmp(&b.schema).then_with(|| a.name.cmp(&b.name)));
    QueryResult::Rows {
        columns: INFO_EVENTS_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows: events.into_iter().map(event_info_row).collect(),
    }
}

fn event_info_row(meta: &EventMeta) -> Row {
    vec![
        meta.schema.clone(),
        meta.name.clone(),
        meta.definer
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(EVENTS_STUB_DEFINER)
            .to_string(),
        meta.schedule_type.clone(),
        meta.execute_at.clone().unwrap_or_default(),
        meta.interval_value.clone().unwrap_or_default(),
        meta.interval_field.clone().unwrap_or_default(),
        meta.starts.clone().unwrap_or_default(),
        meta.ends.clone().unwrap_or_default(),
        meta.status.clone(),
        meta.on_completion
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(EVENTS_DEFAULT_ON_COMPLETION)
            .to_string(),
        meta.last_executed.clone().unwrap_or_default(),
        meta.comment.clone().unwrap_or_default(),
    ]
}

/// Stub `information_schema.TRIGGERS` rows.
pub fn scan_information_schema_triggers(session: &Session) -> QueryResult {
    let rows: Vec<Row> = session
        .catalog
        .iter_triggers()
        .map(|t| {
            let event = match t.event {
                rusql_core::TriggerEvent::Insert => "INSERT",
                rusql_core::TriggerEvent::Update => "UPDATE",
                rusql_core::TriggerEvent::Delete => "DELETE",
            };
            let timing = match t.timing {
                rusql_core::TriggerTiming::Before => "BEFORE",
                rusql_core::TriggerTiming::After => "AFTER",
            };
            vec![
                t.schema.clone(),
                t.name.clone(),
                event.into(),
                t.table.clone(),
                timing.into(),
                t.body.join("; "),
            ]
        })
        .collect();
    QueryResult::Rows {
        columns: INFO_TRIGGERS_COLUMNS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rows,
    }
}

pub fn is_information_schema_table(name: &str) -> Option<&'static str> {
    match name {
        "information_schema.tables" => Some("tables"),
        "information_schema.columns" => Some("columns"),
        "information_schema.SCHEMATA" | "information_schema.schemata" => Some("schemata"),
        "information_schema.STATISTICS" | "information_schema.statistics" => Some("statistics"),
        "information_schema.VIEWS" | "information_schema.views" => Some("views"),
        "information_schema.KEY_COLUMN_USAGE" | "information_schema.key_column_usage" => {
            Some("key_column_usage")
        }
        "information_schema.ROUTINES" | "information_schema.routines" => Some("routines"),
        "information_schema.TRIGGERS" | "information_schema.triggers" => Some("triggers"),
        "information_schema.EVENTS" | "information_schema.events" => Some("events"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::ColumnDef;
    use rusql_storage::HeapEngine;

    fn pk_col(name: &str) -> ColumnDef {
        ColumnDef {
            name: name.into(),
            data_type: "INT".into(),
            nullable: false,
            primary_key: true,
            auto_increment: false,
            collation: None,
        }
    }

    #[test]
    fn describe_primary_key_metadata() {
        let meta = TableMeta {
            name: "pk_t".into(),
            schema: "rusql".into(),
            columns: vec![
                pk_col("id"),
                ColumnDef {
                    name: "label".into(),
                    data_type: "VARCHAR(16)".into(),
                    nullable: false,
                    primary_key: false,
                    auto_increment: false,
                    collation: None,
                },
            ],
            auto_increment_next: None,
            ..Default::default()
        };
        match describe_table(&meta) {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows[0][2], "NO");
                assert_eq!(rows[0][3], "PRI");
                assert_eq!(rows[1][2], "NO");
                assert_eq!(rows[1][3], "");
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn show_create_shape() {
        let meta = TableMeta {
            name: "t".into(),
            schema: "rusql".into(),
            columns: vec![
                ColumnDef::new("id", "int"),
                ColumnDef::new("name", "varchar(32)"),
            ],
            auto_increment_next: None,
            ..Default::default()
        };
        match show_create_table(&meta) {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec!["Table".to_string(), "Create Table".to_string()]
                );
                assert_eq!(rows[0][0], "t");
                assert!(rows[0][1].contains("CREATE TABLE `t`"));
                assert!(rows[0][1].contains("`id` INT"));
                assert!(rows[0][1].contains("`name` VARCHAR(32)"));
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn show_create_view_shape() {
        let meta = ViewMeta {
            name: "v_ids".into(),
            sql: "SELECT id FROM vt".into(),
        };
        match show_create_view(&meta) {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec![
                        "View".to_string(),
                        "Create View".to_string(),
                        "character_set_client".to_string(),
                        "collation_connection".to_string(),
                    ]
                );
                assert_eq!(rows[0][0], "v_ids");
                assert!(rows[0][1].contains("CREATE VIEW `v_ids` AS"));
                assert!(rows[0][1].contains("SELECT id FROM vt"));
                assert_eq!(rows[0][2], DEFAULT_CHARSET);
                assert_eq!(rows[0][3], DEFAULT_COLLATION);
            }
            other => panic!("expected rows, got {other:?}"),
        }
    }

    #[test]
    fn describe_shape() {
        let meta = TableMeta {
            name: "t".into(),
            schema: "rusql".into(),
            columns: vec![ColumnDef::new("id", "INT")],
            auto_increment_next: None,
            ..Default::default()
        };
        match describe_table(&meta) {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns[0], "Field");
                assert_eq!(rows[0][0], "id");
                assert_eq!(rows[0][1], "int");
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn show_index_lists_primary_and_secondary() {
        let mut eng = HeapEngine::new();
        eng.create_table(TableMeta {
            name: "idx_t".into(),
            schema: "rusql".into(),
            columns: vec![pk_col("id"), ColumnDef::new("name", "VARCHAR(32)")],
            auto_increment_next: None,
            ..Default::default()
        })
        .unwrap();
        eng.create_index(rusql_core::IndexMeta::single_column(
            "idx_name", "idx_t", "name",
        ))
        .unwrap();

        let mut session = rusql_core::Session::new(1, "root");
        session.catalog.create_table(TableMeta {
            name: "idx_t".into(),
            schema: "rusql".into(),
            columns: vec![pk_col("id"), ColumnDef::new("name", "VARCHAR(32)")],
            auto_increment_next: None,
            ..Default::default()
        });
        match show_index_for_table(&eng, &session, "idx_t").unwrap() {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(columns[2], "Key_name");
                assert_eq!(columns[3], "Seq_in_index");
                assert_eq!(columns[4], "Column_name");
                assert!(rows.iter().any(|r| r[2] == "PRIMARY" && r[4] == "id"));
                assert!(rows.iter().any(|r| r[2] == "idx_name" && r[4] == "name"));
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn info_schema_schemata_and_statistics() {
        let mut eng = HeapEngine::new();
        eng.create_table(TableMeta {
            name: "idx_t".into(),
            schema: "rusql".into(),
            columns: vec![pk_col("id"), ColumnDef::new("name", "VARCHAR(32)")],
            auto_increment_next: None,
            ..Default::default()
        })
        .unwrap();
        eng.create_index(rusql_core::IndexMeta::single_column(
            "idx_name", "idx_t", "name",
        ))
        .unwrap();

        match scan_information_schema_schemata(&[DEFAULT_SCHEMA.into()]) {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows[0][0], "rusql");
            }
            _ => panic!("expected rows"),
        }

        let mut session = rusql_core::Session::new(1, "root");
        session.catalog.create_table(TableMeta {
            name: "idx_t".into(),
            schema: "rusql".into(),
            columns: vec![pk_col("id"), ColumnDef::new("name", "VARCHAR(32)")],
            auto_increment_next: None,
            ..Default::default()
        });
        match scan_information_schema_statistics(&eng, &session).unwrap() {
            QueryResult::Rows { rows, .. } => {
                assert!(rows.iter().any(|r| r[2] == "PRIMARY" && r[4] == "id"));
                assert!(rows.iter().any(|r| r[2] == "idx_name" && r[4] == "name"));
            }
            _ => panic!("expected rows"),
        }
    }

    #[test]
    fn info_schema_tables() {
        let mut eng = HeapEngine::new();
        eng.create_table(TableMeta {
            name: "a".into(),
            schema: "rusql".into(),
            columns: vec![ColumnDef::new("x", "INT")],
            auto_increment_next: None,
            ..Default::default()
        })
        .unwrap();
        let session = rusql_core::Session::new(1, "root");
        match scan_information_schema_tables(&eng, &session, DEFAULT_SCHEMA) {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec![
                        "rusql".to_string(),
                        "a".to_string(),
                        "BASE TABLE".to_string(),
                    ]]
                );
            }
            _ => panic!("expected rows"),
        }
    }

    fn sample_event(name: &str, comment: Option<&str>) -> EventMeta {
        EventMeta {
            schema: DEFAULT_SCHEMA.into(),
            name: name.into(),
            schedule_type: "RECURRING".into(),
            execute_at: None,
            interval_value: Some("1".into()),
            interval_field: Some("HOUR".into()),
            status: "ENABLED".into(),
            body: "SELECT 1".into(),
            last_executed: None,
            starts: None,
            ends: None,
            definer: None,
            on_completion: None,
            comment: comment.map(str::to_string),
        }
    }

    #[test]
    fn information_schema_events_empty_and_catalog() {
        assert_eq!(
            is_information_schema_table("information_schema.EVENTS"),
            Some("events")
        );
        assert_eq!(
            is_information_schema_table("information_schema.events"),
            Some("events")
        );

        let session = rusql_core::Session::new(1, "root");
        match scan_information_schema_events(&session) {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    INFO_EVENTS_COLUMNS
                        .iter()
                        .map(|s| (*s).to_string())
                        .collect::<Vec<_>>()
                );
                assert!(rows.is_empty());
            }
            other => panic!("empty EVENTS must return rows, got {other:?}"),
        }

        let mut session = rusql_core::Session::new(1, "root");
        session.catalog.create_event(sample_event("e", Some("hi")));
        match scan_information_schema_events(&session) {
            QueryResult::Rows { columns, rows } => {
                let name_i = columns.iter().position(|c| c == "EVENT_NAME").unwrap();
                let comment_i = columns.iter().position(|c| c == "EVENT_COMMENT").unwrap();
                let last_i = columns.iter().position(|c| c == "LAST_EXECUTED").unwrap();
                let definer_i = columns.iter().position(|c| c == "DEFINER").unwrap();
                let on_comp_i = columns.iter().position(|c| c == "ON_COMPLETION").unwrap();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][name_i], "e");
                assert_eq!(rows[0][comment_i], "hi");
                assert_eq!(rows[0][last_i], "");
                assert_eq!(rows[0][definer_i], EVENTS_STUB_DEFINER);
                assert_eq!(rows[0][on_comp_i], EVENTS_DEFAULT_ON_COMPLETION);
            }
            other => panic!("expected EVENTS catalog row, got {other:?}"),
        }
    }
}

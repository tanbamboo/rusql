//! Catalog, session, and type system for rusql.

mod collation;
mod privileges;
mod processlist;
mod programs;
mod types;

pub use collation::{corpus, Collation, DEFAULT_CHARSET, DEFAULT_COLLATION};

pub use privileges::{
    parse_account_ddl, Account, AccountDdl, GrantRecord, GrantTarget, Privilege, PrivilegeStore,
    UserAccountRecord, AUTH_PLUGIN_CACHING_SHA2, AUTH_PLUGIN_NATIVE,
};
pub use processlist::{ConnectionRegistry, ProcessListRow};
pub use programs::{
    EventMeta, FunctionMeta, ProcedureMeta, ProgramStore, TriggerEvent, TriggerMeta, TriggerTiming,
};
pub use types::{column_type_display, data_type_name, normalize_column_type, type_base};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Default logical database name (MySQL `rusql` schema).
pub const DEFAULT_SCHEMA: &str = "rusql";

/// Per-schema character set and collation (M114).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseMeta {
    pub character_set: String,
    pub collation: String,
}

impl DatabaseMeta {
    /// rusql documented defaults (`utf8mb4` / `utf8mb4_unicode_ci`).
    pub fn documented_default() -> Self {
        Self {
            character_set: DEFAULT_CHARSET.to_string(),
            collation: DEFAULT_COLLATION.name().to_string(),
        }
    }
}

fn default_schema() -> String {
    DEFAULT_SCHEMA.to_string()
}

/// Storage map key: bare table name in `rusql`, `schema.table` otherwise (WAL backward compatible).
pub fn table_storage_key(schema: &str, table: &str) -> String {
    if schema == DEFAULT_SCHEMA {
        table.to_string()
    } else {
        format!("{schema}.{table}")
    }
}

/// Column definition in catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
    /// `YES` in DESCRIBE when true (MySQL default: nullable).
    #[serde(default = "default_nullable")]
    pub nullable: bool,
    #[serde(default)]
    pub primary_key: bool,
    #[serde(default)]
    pub auto_increment: bool,
    /// Column collation when `COLLATE` is specified (M62).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collation: Option<Collation>,
}

fn default_nullable() -> bool {
    true
}

impl ColumnDef {
    pub fn new(name: impl Into<String>, data_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            data_type: data_type.into(),
            nullable: true,
            primary_key: false,
            auto_increment: false,
            collation: None,
        }
    }

    /// Effective collation for string compare/sort on this column.
    pub fn effective_collation(&self) -> Collation {
        self.collation.unwrap_or(DEFAULT_COLLATION)
    }
}

/// Referential constraint metadata (FOREIGN KEY).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignKeyMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub referenced_schema: String,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    /// `RESTRICT`, `NO ACTION`, `CASCADE`, …
    #[serde(default = "default_fk_action")]
    pub on_delete: String,
    #[serde(default = "default_fk_action")]
    pub on_update: String,
}

fn default_fk_action() -> String {
    "RESTRICT".to_string()
}

impl ForeignKeyMeta {
    pub fn constraint_name(&self, table: &str, index: usize) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| format!("{table}_ibfk_{}", index + 1))
    }
}

/// Table metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableMeta {
    pub name: String,
    #[serde(default = "default_schema")]
    pub schema: String,
    pub columns: Vec<ColumnDef>,
    /// Next AUTO_INCREMENT value (MySQL-style); `None` if table has no AI column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_increment_next: Option<u64>,
    #[serde(default)]
    pub foreign_keys: Vec<ForeignKeyMeta>,
}

impl Default for TableMeta {
    fn default() -> Self {
        Self {
            name: String::new(),
            schema: DEFAULT_SCHEMA.to_string(),
            columns: Vec::new(),
            auto_increment_next: None,
            foreign_keys: Vec::new(),
        }
    }
}

/// View metadata (read-only SELECT definition).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewMeta {
    pub name: String,
    pub sql: String,
}

/// Secondary index metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexMeta {
    pub name: String,
    pub table: String,
    pub columns: Vec<String>,
}

impl IndexMeta {
    pub fn new(name: impl Into<String>, table: impl Into<String>, columns: Vec<String>) -> Self {
        Self {
            name: name.into(),
            table: table.into(),
            columns,
        }
    }

    pub fn single_column(
        name: impl Into<String>,
        table: impl Into<String>,
        column: impl Into<String>,
    ) -> Self {
        Self::new(name, table, vec![column.into()])
    }
}

/// In-memory database catalog (MVP).
#[derive(Debug, Default)]
pub struct Catalog {
    tables: HashMap<String, TableMeta>,
    views: HashMap<String, ViewMeta>,
    procedures: HashMap<String, ProcedureMeta>,
    functions: HashMap<String, FunctionMeta>,
    triggers: HashMap<String, TriggerMeta>,
    events: HashMap<String, EventMeta>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_table(&mut self, meta: TableMeta) {
        let key = table_storage_key(&meta.schema, &meta.name);
        self.tables.insert(key, meta);
    }

    pub fn get_table(&self, name: &str) -> Option<&TableMeta> {
        self.tables.get(name)
    }

    pub fn drop_table(&mut self, name: &str) {
        self.tables.remove(name);
    }

    pub fn table_names(&self) -> impl Iterator<Item = &String> {
        self.tables.keys()
    }

    pub fn iter_tables(&self) -> impl Iterator<Item = &TableMeta> {
        self.tables.values()
    }

    pub fn create_view(&mut self, meta: ViewMeta) {
        self.views.insert(meta.name.clone(), meta);
    }

    pub fn get_view(&self, name: &str) -> Option<&ViewMeta> {
        self.views.get(name)
    }

    pub fn view_names(&self) -> impl Iterator<Item = &String> {
        self.views.keys()
    }

    pub fn is_view(&self, name: &str) -> bool {
        self.views.contains_key(name)
    }

    pub fn create_procedure(&mut self, meta: ProcedureMeta) {
        let key = format!("{}.{}", meta.schema, meta.name);
        self.procedures.insert(key, meta);
    }

    pub fn get_procedure(&self, schema: &str, name: &str) -> Option<&ProcedureMeta> {
        self.procedures.get(&format!("{schema}.{name}"))
    }

    pub fn drop_procedure(&mut self, schema: &str, name: &str) {
        self.procedures.remove(&format!("{schema}.{name}"));
    }

    pub fn iter_procedures(&self) -> impl Iterator<Item = &ProcedureMeta> {
        self.procedures.values()
    }

    pub fn create_function(&mut self, meta: FunctionMeta) {
        let key = format!("{}.{}", meta.schema, meta.name);
        self.functions.insert(key, meta);
    }

    pub fn get_function(&self, schema: &str, name: &str) -> Option<&FunctionMeta> {
        self.functions
            .values()
            .find(|f| f.schema == schema && f.name.eq_ignore_ascii_case(name))
    }

    pub fn drop_function(&mut self, schema: &str, name: &str) {
        let key = self
            .functions
            .iter()
            .find(|(_, f)| f.schema == schema && f.name.eq_ignore_ascii_case(name))
            .map(|(k, _)| k.clone());
        if let Some(key) = key {
            self.functions.remove(&key);
        }
    }

    pub fn iter_functions(&self) -> impl Iterator<Item = &FunctionMeta> {
        self.functions.values()
    }

    pub fn create_event(&mut self, meta: EventMeta) {
        let key = format!("{}.{}", meta.schema, meta.name);
        self.events.insert(key, meta);
    }

    pub fn get_event(&self, schema: &str, name: &str) -> Option<&EventMeta> {
        self.events
            .values()
            .find(|e| e.schema == schema && e.name.eq_ignore_ascii_case(name))
    }

    pub fn drop_event(&mut self, schema: &str, name: &str) {
        let key = self
            .events
            .iter()
            .find(|(_, e)| e.schema == schema && e.name.eq_ignore_ascii_case(name))
            .map(|(k, _)| k.clone());
        if let Some(key) = key {
            self.events.remove(&key);
        }
    }

    pub fn iter_events(&self) -> impl Iterator<Item = &EventMeta> {
        self.events.values()
    }

    pub fn create_trigger(&mut self, meta: TriggerMeta) {
        let key = format!("{}.{}", meta.schema, meta.name);
        self.triggers.insert(key, meta);
    }

    pub fn get_trigger(&self, schema: &str, name: &str) -> Option<&TriggerMeta> {
        self.triggers.get(&format!("{schema}.{name}"))
    }

    pub fn drop_trigger(&mut self, schema: &str, name: &str) {
        self.triggers.remove(&format!("{schema}.{name}"));
    }

    pub fn triggers_for_table(
        &self,
        schema: &str,
        table: &str,
        timing: TriggerTiming,
        event: TriggerEvent,
    ) -> Vec<&TriggerMeta> {
        self.triggers
            .values()
            .filter(|t| {
                t.schema == schema && t.table == table && t.timing == timing && t.event == event
            })
            .collect()
    }

    pub fn iter_triggers(&self) -> impl Iterator<Item = &TriggerMeta> {
        self.triggers.values()
    }
}

/// Client session state.
#[derive(Debug)]
pub struct Session {
    pub id: u64,
    pub user: String,
    /// Client host pattern (`%` when unknown).
    pub host: String,
    /// Current default database (`USE db`).
    pub database: String,
    pub catalog: Catalog,
    /// Active connection registry for SHOW PROCESSLIST (server-only).
    pub process_list: Option<Arc<ConnectionRegistry>>,
    /// First generated `AUTO_INCREMENT` value of the last successful `INSERT` on this connection.
    /// `0` until an INSERT generates an id (MySQL `LAST_INSERT_ID()` / OK-packet field).
    pub last_insert_id: u64,
    /// Affected-row count of the last statement (`ROW_COUNT()`). `-1` after a result-set
    /// statement (MySQL) and before any statement on a new or reset connection.
    pub row_count: i64,
    /// `FOUND_ROWS()`: last SELECT result size, or un-LIMITed size after `SQL_CALC_FOUND_ROWS`,
    /// or last DML affected-row count. `0` on a new or reset connection.
    pub found_rows: i64,
    /// Set while executing a rewritten `SELECT SQL_CALC_FOUND_ROWS` statement.
    pub sql_calc_found_rows: bool,
    /// In-memory `SET @@` overlay for this connection (not WAL).
    pub session_vars: HashMap<String, String>,
    /// In-memory user variables (`@foo`) for this connection (not WAL).
    pub user_vars: HashMap<String, String>,
}

impl Session {
    pub fn new(id: u64, user: impl Into<String>) -> Self {
        Self {
            id,
            user: user.into(),
            host: "%".into(),
            database: DEFAULT_SCHEMA.into(),
            catalog: Catalog::new(),
            process_list: None,
            last_insert_id: 0,
            row_count: -1,
            found_rows: 0,
            sql_calc_found_rows: false,
            session_vars: HashMap::new(),
            user_vars: HashMap::new(),
        }
    }

    /// Restore documented `@@` stub defaults and clear `@foo` (`COM_RESET_CONNECTION` / `COM_CHANGE_USER`).
    pub fn clear_session_vars(&mut self) {
        self.session_vars.clear();
        self.user_vars.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_create_and_lookup() {
        let mut cat = Catalog::new();
        cat.create_table(TableMeta {
            name: "users".into(),
            schema: DEFAULT_SCHEMA.into(),
            columns: vec![ColumnDef::new("id", "INT")],
            auto_increment_next: None,
            ..Default::default()
        });
        assert!(cat.get_table("users").is_some());
    }
}

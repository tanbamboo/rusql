//! Catalog, session, and type system for rusql.

mod privileges;
mod processlist;
mod programs;
mod types;

pub use privileges::{
    parse_account_ddl, Account, AccountDdl, GrantRecord, GrantTarget, Privilege, PrivilegeStore,
    UserAccountRecord, AUTH_PLUGIN_CACHING_SHA2, AUTH_PLUGIN_NATIVE,
};
pub use processlist::{ConnectionRegistry, ProcessListRow};
pub use programs::{ProcedureMeta, ProgramStore, TriggerEvent, TriggerMeta, TriggerTiming};
pub use types::{column_type_display, data_type_name, normalize_column_type, type_base};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Default logical database name (MySQL `rusql` schema).
pub const DEFAULT_SCHEMA: &str = "rusql";

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
        }
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
    triggers: HashMap<String, TriggerMeta>,
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
        }
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

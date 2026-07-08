//! Catalog, session, and type system for rusql.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
        }
    }
}

/// Table metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableMeta {
    pub name: String,
    pub columns: Vec<ColumnDef>,
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
    pub column: String,
}

/// In-memory database catalog (MVP).
#[derive(Debug, Default)]
pub struct Catalog {
    tables: HashMap<String, TableMeta>,
    views: HashMap<String, ViewMeta>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_table(&mut self, meta: TableMeta) {
        self.tables.insert(meta.name.clone(), meta);
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
}

/// Client session state.
#[derive(Debug)]
pub struct Session {
    pub id: u64,
    pub user: String,
    /// Current default database (`USE db`).
    pub database: String,
    pub catalog: Catalog,
}

impl Session {
    pub fn new(id: u64, user: impl Into<String>) -> Self {
        Self {
            id,
            user: user.into(),
            database: "rusql".into(),
            catalog: Catalog::new(),
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
            columns: vec![ColumnDef::new("id", "INT")],
        });
        assert!(cat.get_table("users").is_some());
    }
}

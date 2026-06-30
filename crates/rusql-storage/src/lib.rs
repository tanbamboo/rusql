//! Storage engine abstraction for rusql.

mod persistent;
mod wal;

use rusql_core::TableMeta;
use std::collections::HashMap;

pub use persistent::PersistentEngine;
pub use wal::WalRecord;

/// Storage-level errors.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("{0}")]
    Message(String),
}

impl StorageError {
    pub fn table_not_found(name: &str) -> Self {
        Self::Message(rusql_i18n::messages::storage_table_not_found(name))
    }
}

/// Row as ordered string values (MVP).
pub type Row = Vec<String>;

/// Storage engine trait.
pub trait StorageEngine: Send + Sync {
    fn create_table(&mut self, meta: TableMeta) -> Result<(), StorageError>;
    fn insert(&mut self, table: &str, row: Row) -> Result<(), StorageError>;
    fn scan(&self, table: &str) -> Result<Vec<Row>, StorageError>;
}

/// In-memory heap storage (MVP).
#[derive(Debug, Default)]
pub struct HeapEngine {
    tables: HashMap<String, Vec<Row>>,
    meta: HashMap<String, TableMeta>,
}

impl HeapEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn table_metas(&self) -> Vec<TableMeta> {
        self.meta.values().cloned().collect()
    }
}

impl StorageEngine for HeapEngine {
    fn create_table(&mut self, meta: TableMeta) -> Result<(), StorageError> {
        let name = meta.name.clone();
        self.meta.insert(name.clone(), meta);
        self.tables.entry(name).or_default();
        Ok(())
    }

    fn insert(&mut self, table: &str, row: Row) -> Result<(), StorageError> {
        if !self.meta.contains_key(table) {
            return Err(StorageError::table_not_found(table));
        }
        self.tables.get_mut(table).unwrap().push(row);
        Ok(())
    }

    fn scan(&self, table: &str) -> Result<Vec<Row>, StorageError> {
        self.tables
            .get(table)
            .cloned()
            .ok_or_else(|| StorageError::table_not_found(table))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::ColumnDef;

    #[test]
    fn heap_insert_and_scan() {
        let mut engine = HeapEngine::new();
        engine
            .create_table(TableMeta {
                name: "t".into(),
                columns: vec![ColumnDef {
                    name: "id".into(),
                    data_type: "INT".into(),
                }],
            })
            .unwrap();
        engine.insert("t", vec!["1".into()]).unwrap();
        assert_eq!(engine.scan("t").unwrap().len(), 1);
    }
}

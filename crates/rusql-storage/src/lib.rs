//! Storage engine abstraction for rusql.

mod btree_index;
mod persistent;
mod wal;

use rusql_core::{IndexMeta, TableMeta};
use std::collections::HashMap;

pub use btree_index::BTreeSecondaryIndex;
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
    fn create_index(&mut self, meta: IndexMeta) -> Result<(), StorageError>;
    /// Point lookup via secondary index; `None` if no index on `column`.
    fn scan_eq(
        &self,
        table: &str,
        column: &str,
        value: &str,
    ) -> Result<Option<Vec<Row>>, StorageError>;
}

fn column_index(meta: &TableMeta, column: &str) -> Result<usize, StorageError> {
    meta.columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(column))
        .ok_or_else(|| StorageError::Message(format!("column '{column}' not found")))
}

/// In-memory heap storage (MVP).
#[derive(Debug, Default)]
pub struct HeapEngine {
    tables: HashMap<String, Vec<Row>>,
    meta: HashMap<String, TableMeta>,
    indexes: HashMap<(String, String), BTreeSecondaryIndex>,
    index_meta: Vec<IndexMeta>,
}

impl HeapEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn table_metas(&self) -> Vec<TableMeta> {
        self.meta.values().cloned().collect()
    }

    pub fn index_metas(&self) -> &[IndexMeta] {
        &self.index_meta
    }

    fn index_key(table: &str, index_name: &str) -> (String, String) {
        (table.to_string(), index_name.to_string())
    }

    fn update_indexes_on_insert(&mut self, table: &str, row: &Row, row_id: u64) {
        for def in &self.index_meta {
            if def.table != table {
                continue;
            }
            if let Some(meta) = self.meta.get(table) {
                if let Ok(col_idx) = column_index(meta, &def.column) {
                    if let Some(val) = row.get(col_idx) {
                        let key = Self::index_key(table, &def.name);
                        if let Some(idx) = self.indexes.get_mut(&key) {
                            idx.insert(val.clone(), row_id);
                        }
                    }
                }
            }
        }
    }

    fn backfill_index(&mut self, def: &IndexMeta) -> Result<(), StorageError> {
        let table_meta = self
            .meta
            .get(&def.table)
            .ok_or_else(|| StorageError::table_not_found(&def.table))?;
        let col_idx = column_index(table_meta, &def.column)?;
        let rows = self.tables.get(&def.table).cloned().unwrap_or_default();
        let key = Self::index_key(&def.table, &def.name);
        let btree = self.indexes.entry(key).or_default();
        for (row_id, row) in rows.iter().enumerate() {
            if let Some(val) = row.get(col_idx) {
                btree.insert(val.clone(), row_id as u64);
            }
        }
        Ok(())
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
        let row_id = self.tables.get(table).map(|t| t.len() as u64).unwrap_or(0);
        self.tables.get_mut(table).unwrap().push(row.clone());
        self.update_indexes_on_insert(table, &row, row_id);
        Ok(())
    }

    fn scan(&self, table: &str) -> Result<Vec<Row>, StorageError> {
        self.tables
            .get(table)
            .cloned()
            .ok_or_else(|| StorageError::table_not_found(table))
    }

    fn create_index(&mut self, meta: IndexMeta) -> Result<(), StorageError> {
        if !self.meta.contains_key(&meta.table) {
            return Err(StorageError::table_not_found(&meta.table));
        }
        let table_meta = self.meta.get(&meta.table).unwrap();
        column_index(table_meta, &meta.column)?;
        let key = Self::index_key(&meta.table, &meta.name);
        if self.indexes.contains_key(&key) {
            return Err(StorageError::Message(format!(
                "index '{}' already exists on table '{}'",
                meta.name, meta.table
            )));
        }
        self.indexes.insert(key, BTreeSecondaryIndex::default());
        self.backfill_index(&meta)?;
        self.index_meta.push(meta);
        Ok(())
    }

    fn scan_eq(
        &self,
        table: &str,
        column: &str,
        value: &str,
    ) -> Result<Option<Vec<Row>>, StorageError> {
        let def = self
            .index_meta
            .iter()
            .find(|d| d.table == table && d.column.eq_ignore_ascii_case(column));
        let Some(def) = def else {
            return Ok(None);
        };
        let rows = self
            .tables
            .get(table)
            .ok_or_else(|| StorageError::table_not_found(table))?;
        let key = Self::index_key(table, &def.name);
        let btree = self
            .indexes
            .get(&key)
            .ok_or_else(|| StorageError::Message(format!("index '{}' missing", def.name)))?;
        let mut out = Vec::new();
        for &row_id in btree.lookup(value) {
            if let Some(row) = rows.get(row_id as usize) {
                out.push(row.clone());
            }
        }
        Ok(Some(out))
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

    #[test]
    fn index_point_lookup() {
        let mut engine = HeapEngine::new();
        engine
            .create_table(TableMeta {
                name: "t".into(),
                columns: vec![
                    ColumnDef {
                        name: "id".into(),
                        data_type: "INT".into(),
                    },
                    ColumnDef {
                        name: "name".into(),
                        data_type: "VARCHAR".into(),
                    },
                ],
            })
            .unwrap();
        engine
            .insert("t", vec!["1".into(), "alice".into()])
            .unwrap();
        engine.insert("t", vec!["2".into(), "bob".into()]).unwrap();
        engine
            .create_index(IndexMeta {
                name: "idx_id".into(),
                table: "t".into(),
                column: "id".into(),
            })
            .unwrap();
        let rows = engine.scan_eq("t", "id", "2").unwrap().unwrap();
        assert_eq!(rows, vec![vec!["2".to_string(), "bob".to_string()]]);
    }
}

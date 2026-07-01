//! Storage engine abstraction for rusql.

mod btree_index;
mod persistent;
mod txn;
mod wal;

use rusql_core::{IndexMeta, TableMeta};
use std::collections::HashMap;

pub use btree_index::BTreeSecondaryIndex;
pub use persistent::PersistentEngine;
pub use txn::{OverlayEngine, TransactionState};
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

/// Optional equality filter for DELETE / UPDATE WHERE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteFilter {
    pub column: String,
    pub value: String,
}

/// Column assignment for UPDATE.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ColumnAssignment {
    pub column: String,
    pub value: String,
}

/// Storage engine trait.
pub trait StorageEngine: Send + Sync {
    fn create_table(&mut self, meta: TableMeta) -> Result<(), StorageError>;
    fn insert(&mut self, table: &str, row: Row) -> Result<(), StorageError>;
    fn scan(&self, table: &str) -> Result<Vec<Row>, StorageError>;
    fn drop_table(&mut self, table: &str) -> Result<(), StorageError>;
    fn delete_rows(
        &mut self,
        table: &str,
        filter: Option<DeleteFilter>,
    ) -> Result<u64, StorageError>;
    fn update_rows(
        &mut self,
        table: &str,
        assignments: &[ColumnAssignment],
        filter: Option<DeleteFilter>,
    ) -> Result<u64, StorageError>;
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

    fn rebuild_indexes_for_table(&mut self, table: &str) -> Result<(), StorageError> {
        let defs: Vec<IndexMeta> = self
            .index_meta
            .iter()
            .filter(|d| d.table == table)
            .cloned()
            .collect();
        for def in defs {
            let key = Self::index_key(table, &def.name);
            if let Some(idx) = self.indexes.get_mut(&key) {
                *idx = BTreeSecondaryIndex::default();
            }
            self.backfill_index(&def)?;
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

    fn drop_table(&mut self, table: &str) -> Result<(), StorageError> {
        if self.meta.remove(table).is_none() {
            return Err(StorageError::table_not_found(table));
        }
        self.tables.remove(table);
        self.index_meta.retain(|d| d.table != table);
        self.indexes.retain(|(t, _), _| t != table);
        Ok(())
    }

    fn delete_rows(
        &mut self,
        table: &str,
        filter: Option<DeleteFilter>,
    ) -> Result<u64, StorageError> {
        if !self.meta.contains_key(table) {
            return Err(StorageError::table_not_found(table));
        }
        let rows = self.tables.get_mut(table).unwrap();
        let before = rows.len();
        if let Some(f) = filter {
            let table_meta = self.meta.get(table).unwrap();
            let col_idx = column_index(table_meta, &f.column)?;
            rows.retain(|r| r.get(col_idx).map(|v| v != &f.value).unwrap_or(true));
        } else {
            rows.clear();
        }
        let deleted = (before - rows.len()) as u64;
        self.rebuild_indexes_for_table(table)?;
        Ok(deleted)
    }

    fn update_rows(
        &mut self,
        table: &str,
        assignments: &[ColumnAssignment],
        filter: Option<DeleteFilter>,
    ) -> Result<u64, StorageError> {
        if !self.meta.contains_key(table) {
            return Err(StorageError::table_not_found(table));
        }
        if assignments.is_empty() {
            return Ok(0);
        }
        let table_meta = self.meta.get(table).unwrap().clone();
        let mut col_indices = Vec::with_capacity(assignments.len());
        for a in assignments {
            col_indices.push((column_index(&table_meta, &a.column)?, a.value.clone()));
        }
        let rows = self.tables.get_mut(table).unwrap();
        let mut updated = 0u64;
        for row in rows.iter_mut() {
            let matches = match &filter {
                None => true,
                Some(f) => {
                    let idx = column_index(&table_meta, &f.column)?;
                    row.get(idx).map(|v| v == &f.value).unwrap_or(false)
                }
            };
            if !matches {
                continue;
            }
            for &(idx, ref val) in &col_indices {
                if idx >= row.len() {
                    return Err(StorageError::Message(format!(
                        "column index out of range for table '{table}'"
                    )));
                }
                row[idx] = val.clone();
            }
            updated += 1;
        }
        self.rebuild_indexes_for_table(table)?;
        Ok(updated)
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

    #[test]
    fn drop_and_delete_rows() {
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
        engine.insert("t", vec!["2".into()]).unwrap();
        let n = engine
            .delete_rows(
                "t",
                Some(DeleteFilter {
                    column: "id".into(),
                    value: "1".into(),
                }),
            )
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(engine.scan("t").unwrap(), vec![vec!["2".to_string()]]);
        engine.drop_table("t").unwrap();
        assert!(engine.scan("t").is_err());
    }
}

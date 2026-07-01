//! Persistent heap engine with WAL backing.

use std::path::{Path, PathBuf};

use rusql_core::{IndexMeta, TableMeta};

use crate::wal::{append_record, replay_into, WalRecord};
use crate::{ColumnAssignment, DeleteFilter, HeapEngine, Row, StorageEngine, StorageError};

/// Heap storage with append-only WAL persistence.
#[derive(Debug)]
pub struct PersistentEngine {
    heap: HeapEngine,
    wal_path: PathBuf,
}

impl PersistentEngine {
    /// Open or create storage in `data_dir` and replay existing WAL.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let data_dir = data_dir.as_ref();
        std::fs::create_dir_all(data_dir)
            .map_err(|e| StorageError::Message(format!("data directory error: {e}")))?;
        let wal_path = data_dir.join("rusql.wal");
        let mut engine = Self {
            heap: HeapEngine::new(),
            wal_path,
        };
        engine.replay()?;
        Ok(engine)
    }

    fn replay(&mut self) -> Result<(), StorageError> {
        replay_into(&self.wal_path, &mut |record| match record {
            WalRecord::CreateTable { name, columns } => {
                self.heap.create_table(TableMeta { name, columns })
            }
            WalRecord::Insert { table, row } => self.heap.insert(&table, row),
            WalRecord::CreateIndex {
                name,
                table,
                column,
            } => self.heap.create_index(IndexMeta {
                name,
                table,
                column,
            }),
            WalRecord::DropTable { name } => self.heap.drop_table(&name),
            WalRecord::DeleteRows {
                table,
                column,
                value,
            } => {
                let filter = match (column, value) {
                    (Some(c), Some(v)) => Some(DeleteFilter {
                        column: c,
                        value: v,
                    }),
                    (None, None) => None,
                    _ => return Err(StorageError::Message("invalid DELETE WAL record".into())),
                };
                self.heap.delete_rows(&table, filter).map(|_| ())
            }
            WalRecord::UpdateRows {
                table,
                assignments,
                where_column,
                where_value,
            } => {
                let filter = match (where_column, where_value) {
                    (Some(c), Some(v)) => Some(DeleteFilter {
                        column: c,
                        value: v,
                    }),
                    (None, None) => None,
                    _ => return Err(StorageError::Message("invalid UPDATE WAL record".into())),
                };
                self.heap
                    .update_rows(&table, &assignments, filter)
                    .map(|_| ())
            }
        })
    }

    /// All table metadata (for seeding session catalog).
    pub fn table_metas(&self) -> Vec<TableMeta> {
        self.heap.table_metas()
    }
}

impl StorageEngine for PersistentEngine {
    fn create_table(&mut self, meta: TableMeta) -> Result<(), StorageError> {
        append_record(&self.wal_path, &WalRecord::from_create(&meta))?;
        self.heap.create_table(meta)
    }

    fn insert(&mut self, table: &str, row: Row) -> Result<(), StorageError> {
        append_record(&self.wal_path, &WalRecord::from_insert(table, row.clone()))?;
        self.heap.insert(table, row)
    }

    fn scan(&self, table: &str) -> Result<Vec<Row>, StorageError> {
        self.heap.scan(table)
    }

    fn drop_table(&mut self, table: &str) -> Result<(), StorageError> {
        append_record(&self.wal_path, &WalRecord::from_drop_table(table))?;
        self.heap.drop_table(table)
    }

    fn delete_rows(
        &mut self,
        table: &str,
        filter: Option<DeleteFilter>,
    ) -> Result<u64, StorageError> {
        append_record(
            &self.wal_path,
            &WalRecord::from_delete(table, filter.as_ref()),
        )?;
        self.heap.delete_rows(table, filter)
    }

    fn update_rows(
        &mut self,
        table: &str,
        assignments: &[ColumnAssignment],
        filter: Option<DeleteFilter>,
    ) -> Result<u64, StorageError> {
        append_record(
            &self.wal_path,
            &WalRecord::from_update(table, assignments, filter.as_ref()),
        )?;
        self.heap.update_rows(table, assignments, filter)
    }

    fn create_index(&mut self, meta: IndexMeta) -> Result<(), StorageError> {
        append_record(&self.wal_path, &WalRecord::from_create_index(&meta))?;
        self.heap.create_index(meta)
    }

    fn scan_eq(
        &self,
        table: &str,
        column: &str,
        value: &str,
    ) -> Result<Option<Vec<Row>>, StorageError> {
        self.heap.scan_eq(table, column, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StorageEngine;
    use rusql_core::ColumnDef;

    fn temp_dir(suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rusql-persist-{}-{}", std::process::id(), suffix))
    }

    #[test]
    fn survives_reopen() {
        let dir = temp_dir("reopen");
        let _ = std::fs::remove_dir_all(&dir);

        {
            let mut e = PersistentEngine::open(&dir).unwrap();
            e.create_table(TableMeta {
                name: "t".into(),
                columns: vec![ColumnDef {
                    name: "id".into(),
                    data_type: "INT".into(),
                }],
            })
            .unwrap();
            e.insert("t", vec!["42".into()]).unwrap();
        }

        let e = PersistentEngine::open(&dir).unwrap();
        let rows = e.scan("t").unwrap();
        assert_eq!(rows, vec![vec!["42".to_string()]]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_survives_reopen() {
        let dir = temp_dir("index");
        let _ = std::fs::remove_dir_all(&dir);

        {
            let mut e = PersistentEngine::open(&dir).unwrap();
            e.create_table(TableMeta {
                name: "t".into(),
                columns: vec![ColumnDef {
                    name: "id".into(),
                    data_type: "INT".into(),
                }],
            })
            .unwrap();
            e.insert("t", vec!["7".into()]).unwrap();
            e.create_index(IndexMeta {
                name: "idx_id".into(),
                table: "t".into(),
                column: "id".into(),
            })
            .unwrap();
        }

        let e = PersistentEngine::open(&dir).unwrap();
        let rows = e.scan_eq("t", "id", "7").unwrap().unwrap();
        assert_eq!(rows, vec![vec!["7".to_string()]]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

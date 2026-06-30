//! Persistent heap engine with WAL backing.

use std::path::{Path, PathBuf};

use rusql_core::TableMeta;

use crate::wal::{append_record, replay_into, WalRecord};
use crate::{HeapEngine, Row, StorageEngine, StorageError};

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StorageEngine;
    use rusql_core::ColumnDef;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("rusql-persist-{}", std::process::id()))
    }

    #[test]
    fn survives_reopen() {
        let dir = temp_dir();
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
}

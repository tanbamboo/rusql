//! Persistent heap engine with WAL backing.

use std::path::{Path, PathBuf};

use rusql_core::{table_storage_key, IndexMeta, TableMeta};

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
        replay_into(&self.wal_path, &mut |record| {
            apply_wal_record(&mut self.heap, record)
        })
    }

    /// Copy committed table state into a heap (for transaction overlay).
    pub fn copy_table_into(
        &self,
        target: &mut HeapEngine,
        table: &str,
    ) -> Result<(), StorageError> {
        let meta = self
            .heap
            .table_metas()
            .into_iter()
            .find(|m| table_storage_key(&m.schema, &m.name) == table)
            .ok_or_else(|| StorageError::table_not_found(table))?;
        target.create_table(meta)?;
        for row in self.heap.scan(table)? {
            target.insert(table, row)?;
        }
        for idx in self
            .heap
            .index_metas()
            .iter()
            .filter(|i| i.table == table)
            .cloned()
        {
            target.create_index(idx)?;
        }
        Ok(())
    }

    /// Flush pending WAL records from a committed transaction.
    pub fn commit_transaction(&mut self, pending: &[WalRecord]) -> Result<(), StorageError> {
        for record in pending {
            append_record(&self.wal_path, record)?;
            apply_wal_record(&mut self.heap, record.clone())?;
        }
        Ok(())
    }

    /// All table metadata (for seeding session catalog).
    pub fn table_metas(&self) -> Vec<TableMeta> {
        self.heap.table_metas()
    }

    pub fn list_databases(&self) -> Vec<String> {
        self.heap.list_databases()
    }
}

/// Apply one WAL record to a heap engine (replay / commit).
pub fn apply_wal_record(heap: &mut HeapEngine, record: WalRecord) -> Result<(), StorageError> {
    match record {
        WalRecord::CreateDatabase { name } => StorageEngine::create_database(heap, &name),
        WalRecord::DropDatabase { name } => StorageEngine::drop_database(heap, &name),
        WalRecord::CreateTable {
            schema,
            name,
            columns,
            auto_increment_next,
        } => heap.create_table(TableMeta {
            name,
            schema,
            columns,
            auto_increment_next,
        }),
        WalRecord::SetAutoIncrement { table, next } => heap.set_auto_increment(&table, next),
        WalRecord::Insert { table, row } => heap.insert(&table, row),
        WalRecord::CreateIndex {
            name,
            table,
            column,
        } => heap.create_index(IndexMeta {
            name,
            table,
            column,
        }),
        WalRecord::DropTable { name } => heap.drop_table(&name),
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
            heap.delete_rows(&table, filter).map(|_| ())
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
            heap.update_rows(&table, &assignments, filter).map(|_| ())
        }
        WalRecord::AddColumn { table, column } => heap.add_column(&table, column),
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

    fn table_names(&self) -> Vec<String> {
        self.heap.table_names()
    }

    fn table_names_in(&self, schema: &str) -> Vec<String> {
        self.heap.table_names_in(schema)
    }

    fn list_databases(&self) -> Vec<String> {
        self.heap.list_databases()
    }

    fn create_database(&mut self, name: &str) -> Result<(), StorageError> {
        append_record(&self.wal_path, &WalRecord::from_create_database(name))?;
        StorageEngine::create_database(&mut self.heap, name)
    }

    fn drop_database(&mut self, name: &str) -> Result<(), StorageError> {
        append_record(&self.wal_path, &WalRecord::from_drop_database(name))?;
        StorageEngine::drop_database(&mut self.heap, name)
    }

    fn index_metas(&self) -> Vec<IndexMeta> {
        self.heap.index_metas().to_vec()
    }

    fn add_column(
        &mut self,
        table: &str,
        column: rusql_core::ColumnDef,
    ) -> Result<(), StorageError> {
        append_record(&self.wal_path, &WalRecord::from_add_column(table, &column))?;
        self.heap.add_column(table, column)
    }

    fn set_auto_increment(&mut self, table: &str, next: u64) -> Result<(), StorageError> {
        append_record(
            &self.wal_path,
            &WalRecord::from_set_auto_increment(table, next),
        )?;
        self.heap.set_auto_increment(table, next)
    }
}

fn read_only_error() -> StorageError {
    StorageError::Message("read-only storage view".into())
}

/// Read-only view over committed storage (for concurrent snapshot reads).
pub struct ReadOnlyEngine<'a>(&'a PersistentEngine);

impl<'a> ReadOnlyEngine<'a> {
    pub fn new(engine: &'a PersistentEngine) -> Self {
        Self(engine)
    }
}

impl StorageEngine for ReadOnlyEngine<'_> {
    fn create_table(&mut self, _meta: TableMeta) -> Result<(), StorageError> {
        Err(read_only_error())
    }

    fn insert(&mut self, _table: &str, _row: Row) -> Result<(), StorageError> {
        Err(read_only_error())
    }

    fn scan(&self, table: &str) -> Result<Vec<Row>, StorageError> {
        self.0.scan(table)
    }

    fn drop_table(&mut self, _table: &str) -> Result<(), StorageError> {
        Err(read_only_error())
    }

    fn delete_rows(
        &mut self,
        _table: &str,
        _filter: Option<DeleteFilter>,
    ) -> Result<u64, StorageError> {
        Err(read_only_error())
    }

    fn update_rows(
        &mut self,
        _table: &str,
        _assignments: &[ColumnAssignment],
        _filter: Option<DeleteFilter>,
    ) -> Result<u64, StorageError> {
        Err(read_only_error())
    }

    fn create_index(&mut self, _meta: IndexMeta) -> Result<(), StorageError> {
        Err(read_only_error())
    }

    fn scan_eq(
        &self,
        table: &str,
        column: &str,
        value: &str,
    ) -> Result<Option<Vec<Row>>, StorageError> {
        self.0.scan_eq(table, column, value)
    }

    fn table_names(&self) -> Vec<String> {
        self.0.table_names()
    }

    fn table_names_in(&self, schema: &str) -> Vec<String> {
        self.0.table_names_in(schema)
    }

    fn list_databases(&self) -> Vec<String> {
        self.0.list_databases()
    }

    fn create_database(&mut self, _name: &str) -> Result<(), StorageError> {
        Err(read_only_error())
    }

    fn drop_database(&mut self, _name: &str) -> Result<(), StorageError> {
        Err(read_only_error())
    }

    fn index_metas(&self) -> Vec<IndexMeta> {
        self.0.index_metas()
    }

    fn add_column(
        &mut self,
        _table: &str,
        _column: rusql_core::ColumnDef,
    ) -> Result<(), StorageError> {
        Err(read_only_error())
    }

    fn set_auto_increment(&mut self, _table: &str, _next: u64) -> Result<(), StorageError> {
        Err(read_only_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StorageEngine, TransactionState};
    use rusql_core::ColumnDef;

    fn temp_dir(suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rusql-persist-{}-{}", std::process::id(), suffix))
    }

    #[test]
    fn commit_transaction_survives_reopen() {
        let dir = temp_dir("commit-reopen");
        let _ = std::fs::remove_dir_all(&dir);

        let pending = {
            let base = PersistentEngine::open(&dir).unwrap();
            let mut txn = TransactionState::new();
            {
                let mut eng = crate::txn::OverlayEngine::new(&base, &mut txn);
                eng.create_table(TableMeta {
                    name: "t".into(),
                    schema: "rusql".into(),
                    columns: vec![ColumnDef::new("id", "INT")],
                    auto_increment_next: None,
                })
                .unwrap();
                eng.insert("t", vec!["42".into()]).unwrap();
            }
            txn.pending_records().to_vec()
        };

        {
            let mut e = PersistentEngine::open(&dir).unwrap();
            e.commit_transaction(&pending).unwrap();
        }

        let e = PersistentEngine::open(&dir).unwrap();
        assert_eq!(e.scan("t").unwrap(), vec![vec!["42".to_string()]]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wal_replay_has_all_inserts() {
        let dir = temp_dir("two-inserts");
        let _ = std::fs::remove_dir_all(&dir);
        {
            let mut e = PersistentEngine::open(&dir).unwrap();
            e.create_table(TableMeta {
                name: "md_t".into(),
                schema: "rusql".into(),
                columns: vec![
                    ColumnDef::new("id", "INT"),
                    ColumnDef::new("name", "VARCHAR(32)"),
                ],
                auto_increment_next: None,
            })
            .unwrap();
            e.insert("md_t", vec!["1".into(), "alice".into()]).unwrap();
            e.insert("md_t", vec!["2".into(), "bob".into()]).unwrap();
        }
        let e = PersistentEngine::open(&dir).unwrap();
        assert_eq!(
            e.scan("md_t").unwrap(),
            vec![
                vec!["1".to_string(), "alice".to_string()],
                vec!["2".to_string(), "bob".to_string()],
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn survives_reopen() {
        let dir = temp_dir("reopen");
        let _ = std::fs::remove_dir_all(&dir);

        {
            let mut e = PersistentEngine::open(&dir).unwrap();
            e.create_table(TableMeta {
                name: "t".into(),
                schema: "rusql".into(),
                columns: vec![ColumnDef::new("id", "INT")],
                auto_increment_next: None,
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
                schema: "rusql".into(),
                columns: vec![ColumnDef::new("id", "INT")],
                auto_increment_next: None,
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

    #[test]
    fn create_database_survives_reopen() {
        let dir = temp_dir("create-db");
        let _ = std::fs::remove_dir_all(&dir);
        {
            let mut e = PersistentEngine::open(&dir).unwrap();
            StorageEngine::create_database(&mut e, "app_db").unwrap();
            e.create_table(TableMeta {
                name: "t".into(),
                schema: "app_db".into(),
                columns: vec![ColumnDef::new("id", "INT")],
                auto_increment_next: None,
            })
            .unwrap();
            e.insert("app_db.t", vec!["1".into()]).unwrap();
        }
        let e = PersistentEngine::open(&dir).unwrap();
        assert!(e.list_databases().iter().any(|d| d == "app_db"));
        assert_eq!(e.scan("app_db.t").unwrap(), vec![vec!["1".to_string()]]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_increment_counter_survives_reopen() {
        let dir = temp_dir("auto-inc");
        let _ = std::fs::remove_dir_all(&dir);
        {
            let mut e = PersistentEngine::open(&dir).unwrap();
            e.create_table(TableMeta {
                name: "ai".into(),
                schema: "rusql".into(),
                columns: vec![
                    ColumnDef {
                        name: "id".into(),
                        data_type: "INT".into(),
                        nullable: false,
                        primary_key: true,
                        auto_increment: true,
                    },
                    ColumnDef::new("name", "VARCHAR(16)"),
                ],
                auto_increment_next: Some(1),
            })
            .unwrap();
            e.insert("ai", vec!["1".into(), "a".into()]).unwrap();
            e.set_auto_increment("ai", 2).unwrap();
        }
        let e = PersistentEngine::open(&dir).unwrap();
        let meta = e
            .table_metas()
            .into_iter()
            .find(|m| m.name == "ai")
            .unwrap();
        assert_eq!(meta.auto_increment_next, Some(2));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

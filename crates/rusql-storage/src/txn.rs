//! Transaction overlay — uncommitted writes isolated per connection.

use std::collections::HashSet;

use rusql_core::{IndexMeta, TableMeta};

use crate::{
    ColumnAssignment, DeleteFilter, HeapEngine, PersistentEngine, Row, StorageEngine, StorageError,
    WalRecord,
};

/// Per-connection uncommitted state.
#[derive(Debug, Default)]
pub struct TransactionState {
    overlay: HeapEngine,
    touched: HashSet<String>,
    pending: Vec<WalRecord>,
}

impl TransactionState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending_records(&self) -> &[WalRecord] {
        &self.pending
    }

    pub fn clear(&mut self) {
        self.overlay = HeapEngine::new();
        self.touched.clear();
        self.pending.clear();
    }
}

/// Storage view: reads from overlay when touched, else base; writes go to overlay + pending WAL.
pub struct OverlayEngine<'a> {
    base: &'a PersistentEngine,
    txn: &'a mut TransactionState,
}

impl<'a> OverlayEngine<'a> {
    pub fn new(base: &'a PersistentEngine, txn: &'a mut TransactionState) -> Self {
        Self { base, txn }
    }

    fn ensure_table(&mut self, table: &str) -> Result<(), StorageError> {
        if self.txn.touched.contains(table) {
            return Ok(());
        }
        if self
            .txn
            .overlay
            .table_metas()
            .iter()
            .any(|m| m.name == table)
        {
            self.txn.touched.insert(table.to_string());
            return Ok(());
        }
        self.base.copy_table_into(&mut self.txn.overlay, table)?;
        self.txn.touched.insert(table.to_string());
        Ok(())
    }

    fn push_pending(&mut self, record: WalRecord) {
        self.txn.pending.push(record);
    }
}

impl StorageEngine for OverlayEngine<'_> {
    fn create_table(&mut self, meta: TableMeta) -> Result<(), StorageError> {
        let name = meta.name.clone();
        self.push_pending(WalRecord::from_create(&meta));
        self.txn.overlay.create_table(meta)?;
        self.txn.touched.insert(name);
        Ok(())
    }

    fn insert(&mut self, table: &str, row: Row) -> Result<(), StorageError> {
        self.ensure_table(table)?;
        self.push_pending(WalRecord::from_insert(table, row.clone()));
        self.txn.overlay.insert(table, row)
    }

    fn scan(&self, table: &str) -> Result<Vec<Row>, StorageError> {
        if self.txn.touched.contains(table) {
            return self.txn.overlay.scan(table);
        }
        self.base.scan(table)
    }

    fn drop_table(&mut self, table: &str) -> Result<(), StorageError> {
        let in_base = self.base.table_metas().iter().any(|m| m.name == table);
        let in_overlay = self
            .txn
            .overlay
            .table_metas()
            .iter()
            .any(|m| m.name == table);
        if !in_base && !in_overlay {
            return Err(StorageError::table_not_found(table));
        }
        if in_base {
            self.ensure_table(table)?;
        }
        self.push_pending(WalRecord::from_drop_table(table));
        let _ = self.txn.overlay.drop_table(table);
        self.txn.touched.insert(table.to_string());
        Ok(())
    }

    fn delete_rows(
        &mut self,
        table: &str,
        filter: Option<DeleteFilter>,
    ) -> Result<u64, StorageError> {
        self.ensure_table(table)?;
        self.push_pending(WalRecord::from_delete(table, filter.as_ref()));
        self.txn.overlay.delete_rows(table, filter)
    }

    fn update_rows(
        &mut self,
        table: &str,
        assignments: &[ColumnAssignment],
        filter: Option<DeleteFilter>,
    ) -> Result<u64, StorageError> {
        self.ensure_table(table)?;
        self.push_pending(WalRecord::from_update(table, assignments, filter.as_ref()));
        self.txn.overlay.update_rows(table, assignments, filter)
    }

    fn create_index(&mut self, meta: IndexMeta) -> Result<(), StorageError> {
        self.ensure_table(&meta.table)?;
        self.push_pending(WalRecord::from_create_index(&meta));
        self.txn.overlay.create_index(meta)
    }

    fn scan_eq(
        &self,
        table: &str,
        column: &str,
        value: &str,
    ) -> Result<Option<Vec<Row>>, StorageError> {
        if self.txn.touched.contains(table) {
            return self.txn.overlay.scan_eq(table, column, value);
        }
        self.base.scan_eq(table, column, value)
    }

    fn table_names(&self) -> Vec<String> {
        let mut names: HashSet<String> = self.base.table_names().into_iter().collect();
        for m in self.txn.overlay.table_metas() {
            names.insert(m.name.clone());
        }
        for t in &self.txn.touched {
            if !self.txn.overlay.table_metas().iter().any(|m| m.name == *t) {
                names.remove(t);
            }
        }
        let mut v: Vec<_> = names.into_iter().collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::ColumnDef;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TXN_TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let n = TXN_TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("rusql-txn-{name}-{n}"))
    }

    #[test]
    fn commit_persists_overlay_writes() {
        let dir = temp_dir("commit");
        let _ = std::fs::remove_dir_all(&dir);
        let mut base = PersistentEngine::open(&dir).unwrap();
        let mut txn = TransactionState::new();
        {
            let mut eng = OverlayEngine::new(&base, &mut txn);
            eng.create_table(TableMeta {
                name: "t".into(),
                columns: vec![ColumnDef::new("id", "INT")],
            })
            .unwrap();
            eng.insert("t", vec!["1".into()]).unwrap();
        }
        base.commit_transaction(&txn.pending).unwrap();
        txn.clear();
        assert_eq!(base.scan("t").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rollback_discards_pending() {
        let dir = temp_dir("rollback");
        let _ = std::fs::remove_dir_all(&dir);
        let base = PersistentEngine::open(&dir).unwrap();
        let mut txn = TransactionState::new();
        {
            let mut eng = OverlayEngine::new(&base, &mut txn);
            eng.create_table(TableMeta {
                name: "t".into(),
                columns: vec![ColumnDef::new("id", "INT")],
            })
            .unwrap();
        }
        assert!(!txn.pending.is_empty());
        txn.clear();
        assert!(base.scan("t").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

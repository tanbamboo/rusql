//! Transaction overlay — uncommitted writes isolated per connection.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use rusql_core::{table_storage_key, IndexMeta, TableMeta, DEFAULT_SCHEMA};

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
    /// Pinned committed rows per table (lazy snapshot on first read in this txn).
    snapshot: RwLock<HashMap<String, Vec<Row>>>,
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
        self.snapshot.write().unwrap().clear();
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
        if let Some(rows) = self.txn.snapshot.read().unwrap().get(table) {
            let _ = self.txn.overlay.delete_rows(table, None)?;
            for row in rows {
                self.txn.overlay.insert(table, row.clone())?;
            }
        }
        self.txn.touched.insert(table.to_string());
        Ok(())
    }

    fn snapshot_rows(&self, table: &str) -> Result<Vec<Row>, StorageError> {
        if let Some(rows) = self.txn.snapshot.read().unwrap().get(table) {
            return Ok(rows.clone());
        }
        let rows = self.base.scan(table)?;
        self.txn
            .snapshot
            .write()
            .unwrap()
            .insert(table.to_string(), rows.clone());
        Ok(rows)
    }

    fn push_pending(&mut self, record: WalRecord) {
        self.txn.pending.push(record);
    }
}

impl StorageEngine for OverlayEngine<'_> {
    fn create_table(&mut self, meta: TableMeta) -> Result<(), StorageError> {
        let key = table_storage_key(&meta.schema, &meta.name);
        self.push_pending(WalRecord::from_create(&meta));
        self.txn.overlay.create_table(meta)?;
        self.txn.touched.insert(key);
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
        self.snapshot_rows(table)
    }

    fn drop_table(&mut self, table: &str) -> Result<(), StorageError> {
        let in_base = self
            .base
            .table_metas()
            .iter()
            .any(|m| table_storage_key(&m.schema, &m.name) == table);
        let in_overlay = self
            .txn
            .overlay
            .table_metas()
            .iter()
            .any(|m| table_storage_key(&m.schema, &m.name) == table);
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

    fn scan_eq_prefix(
        &self,
        table: &str,
        eq: &[(&str, &str)],
    ) -> Result<Option<Vec<Row>>, StorageError> {
        if self.txn.touched.contains(table) {
            return self.txn.overlay.scan_eq_prefix(table, eq);
        }
        self.base.scan_eq_prefix(table, eq)
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
        self.scan_eq_prefix(table, &[(column, value)])
    }

    fn scan_range(
        &self,
        table: &str,
        column: &str,
        low: &str,
        high: &str,
    ) -> Result<Option<Vec<Row>>, StorageError> {
        if self.txn.touched.contains(table) {
            return self.txn.overlay.scan_range(table, column, low, high);
        }
        self.base.scan_range(table, column, low, high)
    }

    fn scan_index_ordered(
        &self,
        table: &str,
        column: &str,
        ascending: bool,
        offset: usize,
        limit: usize,
    ) -> Result<Option<Vec<Row>>, StorageError> {
        if self.txn.touched.contains(table) {
            return self
                .txn
                .overlay
                .scan_index_ordered(table, column, ascending, offset, limit);
        }
        self.base
            .scan_index_ordered(table, column, ascending, offset, limit)
    }

    fn row_count(&self, table: &str) -> Result<u64, StorageError> {
        if self.txn.touched.contains(table) {
            return self.txn.overlay.row_count(table);
        }
        self.base.row_count(table)
    }

    fn table_names(&self) -> Vec<String> {
        let mut names: HashSet<String> = self.base.table_names().into_iter().collect();
        for m in self.txn.overlay.table_metas() {
            names.insert(table_storage_key(&m.schema, &m.name));
        }
        for t in &self.txn.touched {
            if !self
                .txn
                .overlay
                .table_metas()
                .iter()
                .any(|m| table_storage_key(&m.schema, &m.name) == *t)
            {
                names.remove(t);
            }
        }
        let mut v: Vec<_> = names.into_iter().collect();
        v.sort();
        v
    }

    fn table_names_in(&self, schema: &str) -> Vec<String> {
        let mut names: HashSet<String> = self.base.table_names_in(schema).into_iter().collect();
        for m in self.txn.overlay.table_metas() {
            if m.schema == schema {
                names.insert(m.name.clone());
            }
        }
        for t in &self.txn.touched {
            let key_prefix_ok = if schema == DEFAULT_SCHEMA {
                !t.contains('.')
            } else {
                t.starts_with(&format!("{schema}."))
            };
            if key_prefix_ok
                && !self
                    .txn
                    .overlay
                    .table_metas()
                    .iter()
                    .any(|m| table_storage_key(&m.schema, &m.name) == *t)
            {
                let bare = t.rsplit_once('.').map(|(_, n)| n).unwrap_or(t.as_str());
                names.remove(bare);
            }
        }
        let mut v: Vec<_> = names.into_iter().collect();
        v.sort();
        v
    }

    fn list_databases(&self) -> Vec<String> {
        let mut dbs: HashSet<String> = self.base.list_databases().into_iter().collect();
        // Overlay may create databases via pending WAL; track via create_database on overlay.
        for name in self.txn.overlay.list_databases() {
            dbs.insert(name);
        }
        let mut v: Vec<_> = dbs.into_iter().collect();
        v.sort();
        v
    }

    fn create_database(&mut self, name: &str) -> Result<(), StorageError> {
        self.push_pending(WalRecord::from_create_database(name));
        StorageEngine::create_database(&mut self.txn.overlay, name)
    }

    fn drop_database(&mut self, name: &str) -> Result<(), StorageError> {
        self.push_pending(WalRecord::from_drop_database(name));
        StorageEngine::drop_database(&mut self.txn.overlay, name)
    }

    fn index_metas(&self) -> Vec<IndexMeta> {
        use std::collections::HashSet;
        let visible: HashSet<String> = self.table_names().into_iter().collect();
        let mut out = Vec::new();
        for idx in StorageEngine::index_metas(self.base) {
            if visible.contains(&idx.table) {
                out.push(idx);
            }
        }
        for idx in self.txn.overlay.index_metas() {
            if visible.contains(&idx.table)
                && !out
                    .iter()
                    .any(|m| m.table == idx.table && m.name == idx.name)
            {
                out.push(idx.clone());
            }
        }
        out.sort_by(|a, b| {
            (a.table.as_str(), a.name.as_str()).cmp(&(b.table.as_str(), b.name.as_str()))
        });
        out
    }

    fn add_column(
        &mut self,
        table: &str,
        column: rusql_core::ColumnDef,
    ) -> Result<(), StorageError> {
        self.ensure_table(table)?;
        self.push_pending(WalRecord::from_add_column(table, &column));
        self.txn.overlay.add_column(table, column)
    }

    fn set_auto_increment(&mut self, table: &str, next: u64) -> Result<(), StorageError> {
        self.ensure_table(table)?;
        self.push_pending(WalRecord::from_set_auto_increment(table, next));
        self.txn.overlay.set_auto_increment(table, next)
    }

    fn drop_column(
        &mut self,
        table: &str,
        column: &str,
        if_exists: bool,
    ) -> Result<(), StorageError> {
        self.ensure_table(table)?;
        self.push_pending(WalRecord::from_drop_column(table, column, if_exists));
        self.txn.overlay.drop_column(table, column, if_exists)
    }

    fn rename_column(
        &mut self,
        table: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<(), StorageError> {
        self.ensure_table(table)?;
        self.push_pending(WalRecord::from_rename_column(table, old_name, new_name));
        self.txn.overlay.rename_column(table, old_name, new_name)
    }

    fn modify_column(
        &mut self,
        table: &str,
        column: rusql_core::ColumnDef,
    ) -> Result<(), StorageError> {
        self.ensure_table(table)?;
        self.push_pending(WalRecord::from_modify_column(table, &column));
        self.txn.overlay.modify_column(table, column)
    }

    fn rename_table(&mut self, old_name: &str, new_name: &str) -> Result<(), StorageError> {
        self.ensure_table(old_name)?;
        self.push_pending(WalRecord::from_rename_table(old_name, new_name));
        self.txn.overlay.rename_table(old_name, new_name)?;
        self.txn.touched.remove(old_name);
        self.txn.touched.insert(new_name.to_string());
        Ok(())
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
                schema: "rusql".into(),
                columns: vec![ColumnDef::new("id", "INT")],
                auto_increment_next: None,
                ..Default::default()
            })
            .unwrap();
            eng.insert("t", vec!["1".into()]).unwrap();
        }
        base.commit_transaction(&txn.pending).unwrap();
        txn.clear();
        assert_eq!(base.scan("t").unwrap().len(), 1);
        drop(base);
        let reopened = PersistentEngine::open(&dir).unwrap();
        assert_eq!(reopened.scan("t").unwrap(), vec![vec!["1".to_string()]]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rollback_does_not_append_wal() {
        let dir = temp_dir("rollback-wal");
        let _ = std::fs::remove_dir_all(&dir);
        let wal_path = dir.join("rusql.wal");
        let base = PersistentEngine::open(&dir).unwrap();
        let mut txn = TransactionState::new();
        {
            let mut eng = OverlayEngine::new(&base, &mut txn);
            eng.create_table(TableMeta {
                name: "t".into(),
                schema: "rusql".into(),
                columns: vec![ColumnDef::new("id", "INT")],
                auto_increment_next: None,
                ..Default::default()
            })
            .unwrap();
            eng.insert("t", vec!["9".into()]).unwrap();
        }
        assert!(!txn.pending.is_empty());
        let wal_before = std::fs::read_to_string(&wal_path).unwrap_or_default();
        txn.clear();
        let wal_after = std::fs::read_to_string(&wal_path).unwrap_or_default();
        assert_eq!(
            wal_before, wal_after,
            "ROLLBACK must not append WAL records"
        );
        assert!(base.scan("t").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_read_isolated_from_concurrent_commit() {
        let dir = temp_dir("snapshot");
        let _ = std::fs::remove_dir_all(&dir);
        let mut base = PersistentEngine::open(&dir).unwrap();
        base.create_table(TableMeta {
            name: "t".into(),
            schema: "rusql".into(),
            columns: vec![ColumnDef::new("id", "INT")],
            auto_increment_next: None,
            ..Default::default()
        })
        .unwrap();
        base.insert("t", vec!["1".into()]).unwrap();

        let mut reader = TransactionState::new();
        {
            let eng = OverlayEngine::new(&base, &mut reader);
            assert_eq!(eng.scan("t").unwrap(), vec![vec!["1".to_string()]]);
        }

        let mut writer = TransactionState::new();
        {
            let mut eng = OverlayEngine::new(&base, &mut writer);
            eng.update_rows(
                "t",
                &[ColumnAssignment {
                    column: "id".into(),
                    value: "2".into(),
                }],
                None,
            )
            .unwrap();
        }
        base.commit_transaction(&writer.pending).unwrap();

        let eng = OverlayEngine::new(&base, &mut reader);
        assert_eq!(
            eng.scan("t").unwrap(),
            vec![vec!["1".to_string()]],
            "reader txn must keep pinned snapshot after writer commit"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

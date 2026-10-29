//! Write-ahead log for rusql persistence (JSON lines).

use std::str::FromStr;

use rusql_core::{
    CheckConstraintMeta, ColumnDef, ForeignKeyMeta, IndexMeta, RangePartitionMeta, TableMeta,
    DEFAULT_SCHEMA,
};
use serde::{Deserialize, Serialize};

fn default_schema() -> String {
    DEFAULT_SCHEMA.to_string()
}

use crate::{ColumnAssignment, Row, StorageError};

/// Prefix in legacy `CreateIndex.column` WAL field for M151 functional keys (not a SQL column name).
pub(crate) const FUNCTIONAL_INDEX_WAL_MARKER: &str = "__RUSQL_FUNCTIONAL_INDEX__:";

/// `CreateIndex.column` marker for an M197 FULLTEXT index. Column names stay in `columns`.
pub(crate) const FULLTEXT_INDEX_WAL_MARKER: &str = "__RUSQL_FULLTEXT_INDEX__";

/// One WAL record (one JSON line in `rusql.wal`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WalRecord {
    CreateDatabase {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        character_set: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        collation: Option<String>,
    },
    DropDatabase {
        name: String,
    },
    CreateTable {
        #[serde(default = "default_schema")]
        schema: String,
        name: String,
        columns: Vec<ColumnDef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auto_increment_next: Option<u64>,
        #[serde(default)]
        foreign_keys: Vec<ForeignKeyMeta>,
        #[serde(default)]
        check_constraints: Vec<CheckConstraintMeta>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        range_partition: Option<RangePartitionMeta>,
    },
    SetAutoIncrement {
        table: String,
        next: u64,
    },
    Insert {
        table: String,
        row: Row,
    },
    CreateIndex {
        name: String,
        table: String,
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        column: Option<String>,
    },
    DropTable {
        name: String,
    },
    DeleteRows {
        table: String,
        column: Option<String>,
        value: Option<String>,
    },
    UpdateRows {
        table: String,
        assignments: Vec<ColumnAssignment>,
        #[serde(rename = "where_column")]
        where_column: Option<String>,
        #[serde(rename = "where_value")]
        where_value: Option<String>,
    },
    AddColumn {
        table: String,
        column: ColumnDef,
    },
    DropColumn {
        table: String,
        column: String,
        if_exists: bool,
    },
    RenameColumn {
        table: String,
        old_name: String,
        new_name: String,
    },
    ModifyColumn {
        table: String,
        column: ColumnDef,
    },
    RenameTable {
        old_name: String,
        new_name: String,
    },
    DropIndex {
        table: String,
        name: String,
        #[serde(default)]
        if_exists: bool,
    },
    /// Prepared XA branch (M162); DML records are applied on `XaCommit` replay.
    XaPrepare {
        xid: String,
        records: Vec<WalRecord>,
    },
    XaCommit {
        xid: String,
    },
    XaRollback {
        xid: String,
    },
}

impl WalRecord {
    pub fn from_create_database(name: &str) -> Self {
        Self::from_create_database_with_charset(name, None, None)
    }

    pub fn from_create_database_with_charset(
        name: &str,
        character_set: Option<&str>,
        collation: Option<&str>,
    ) -> Self {
        Self::CreateDatabase {
            name: name.to_string(),
            character_set: character_set.map(|s| s.to_string()),
            collation: collation.map(|s| s.to_string()),
        }
    }

    pub fn from_drop_database(name: &str) -> Self {
        Self::DropDatabase {
            name: name.to_string(),
        }
    }

    pub fn from_create(meta: &TableMeta) -> Self {
        Self::CreateTable {
            schema: meta.schema.clone(),
            name: meta.name.clone(),
            columns: meta.columns.clone(),
            auto_increment_next: meta.auto_increment_next,
            foreign_keys: meta.foreign_keys.clone(),
            check_constraints: meta.check_constraints.clone(),
            range_partition: meta.range_partition.clone(),
        }
    }

    pub fn from_set_auto_increment(table: &str, next: u64) -> Self {
        Self::SetAutoIncrement {
            table: table.to_string(),
            next,
        }
    }

    pub fn from_insert(table: &str, row: Row) -> Self {
        Self::Insert {
            table: table.to_string(),
            row,
        }
    }

    pub fn from_create_index(meta: &IndexMeta) -> Self {
        if meta.fulltext {
            return Self::CreateIndex {
                name: meta.name.clone(),
                table: meta.table.clone(),
                columns: meta.columns.clone(),
                column: Some(FULLTEXT_INDEX_WAL_MARKER.to_string()),
            };
        }
        if let Some(ref key) = meta.functional_key {
            return Self::CreateIndex {
                name: meta.name.clone(),
                table: meta.table.clone(),
                columns: Vec::new(),
                column: Some(format!("{FUNCTIONAL_INDEX_WAL_MARKER}{key}")),
            };
        }
        if meta.columns.len() == 1 {
            return Self::CreateIndex {
                name: meta.name.clone(),
                table: meta.table.clone(),
                columns: Vec::new(),
                column: Some(meta.columns[0].clone()),
            };
        }
        Self::CreateIndex {
            name: meta.name.clone(),
            table: meta.table.clone(),
            columns: meta.columns.clone(),
            column: None,
        }
    }

    pub fn from_drop_table(name: &str) -> Self {
        Self::DropTable {
            name: name.to_string(),
        }
    }

    pub fn from_delete(table: &str, filter: Option<&crate::DeleteFilter>) -> Self {
        Self::DeleteRows {
            table: table.to_string(),
            column: filter.map(|f| f.column.clone()),
            value: filter.map(|f| f.value.clone()),
        }
    }

    pub fn from_update(
        table: &str,
        assignments: &[ColumnAssignment],
        filter: Option<&crate::DeleteFilter>,
    ) -> Self {
        Self::UpdateRows {
            table: table.to_string(),
            assignments: assignments.to_vec(),
            where_column: filter.map(|f| f.column.clone()),
            where_value: filter.map(|f| f.value.clone()),
        }
    }

    pub fn from_add_column(table: &str, column: &ColumnDef) -> Self {
        Self::AddColumn {
            table: table.to_string(),
            column: column.clone(),
        }
    }

    pub fn from_drop_column(table: &str, column: &str, if_exists: bool) -> Self {
        Self::DropColumn {
            table: table.to_string(),
            column: column.to_string(),
            if_exists,
        }
    }

    pub fn from_rename_column(table: &str, old_name: &str, new_name: &str) -> Self {
        Self::RenameColumn {
            table: table.to_string(),
            old_name: old_name.to_string(),
            new_name: new_name.to_string(),
        }
    }

    pub fn from_modify_column(table: &str, column: &ColumnDef) -> Self {
        Self::ModifyColumn {
            table: table.to_string(),
            column: column.clone(),
        }
    }

    pub fn from_rename_table(old_name: &str, new_name: &str) -> Self {
        Self::RenameTable {
            old_name: old_name.to_string(),
            new_name: new_name.to_string(),
        }
    }
}

/// WAL durability policy (maps to MySQL `innodb_flush_log_at_trx_commit` semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WalSyncPolicy {
    /// `fsync` after every autocommit statement and transaction commit (default, safest).
    #[default]
    Always,
    /// `fsync` only at transaction commit; autocommit statements skip per-record sync.
    Batch,
    /// Never `fsync` — highest throughput, data loss on crash.
    None,
}

impl WalSyncPolicy {
    /// Whether to `fsync` after each autocommit WAL append.
    pub fn sync_after_autocommit(self) -> bool {
        matches!(self, Self::Always)
    }

    /// Whether to `fsync` after a committed transaction batch.
    pub fn sync_after_transaction(self) -> bool {
        !matches!(self, Self::None)
    }
}

impl FromStr for WalSyncPolicy {
    type Err = StorageError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "always" => Ok(Self::Always),
            "batch" => Ok(Self::Batch),
            "none" => Ok(Self::None),
            other => Err(StorageError::Message(format!(
                "invalid wal-sync policy '{other}' (expected always|batch|none)"
            ))),
        }
    }
}

/// Flush WAL file to durable storage.
pub fn sync_wal(path: &std::path::Path) -> Result<(), StorageError> {
    let file = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|e| StorageError::Message(format!("wal open error: {e}")))?;
    file.sync_data()
        .map_err(|e| StorageError::Message(format!("wal sync error: {e}")))?;
    Ok(())
}

/// Append one record to the WAL file; optionally `fsync` when `sync` is true.
pub fn append_record(
    path: &std::path::Path,
    record: &WalRecord,
    sync: bool,
) -> Result<(), StorageError> {
    use std::io::Write;
    let line = serde_json::to_string(record)
        .map_err(|e| StorageError::Message(format!("wal encode error: {e}")))?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| StorageError::Message(format!("wal open error: {e}")))?;
    writeln!(file, "{line}").map_err(|e| StorageError::Message(format!("wal write error: {e}")))?;
    if sync {
        sync_wal(path)?;
    }
    Ok(())
}

/// Replay complete WAL lines into a heap engine (without re-appending).
///
/// A trailing partial line (no newline) is a torn write from a crash. It is
/// discarded and the file is truncated back to the last complete line, then
/// synced. A complete line that does not decode is corruption: recovery fails
/// and the file is left unchanged. This is not an InnoDB redo or `.ibd` replay.
pub fn replay_into(
    path: &std::path::Path,
    apply: &mut dyn FnMut(WalRecord) -> Result<(), StorageError>,
) -> Result<(), StorageError> {
    use std::io::{BufRead, BufReader};
    if !path.exists() {
        return Ok(());
    }
    let file_len = std::fs::metadata(path)
        .map_err(|e| StorageError::Message(format!("wal read error: {e}")))?
        .len();
    let durable_end = {
        let file = std::fs::File::open(path)
            .map_err(|e| StorageError::Message(format!("wal read error: {e}")))?;
        let mut reader = BufReader::new(file);
        let mut buf = Vec::new();
        let mut durable_end: u64 = 0;
        loop {
            buf.clear();
            let n = reader
                .read_until(b'\n', &mut buf)
                .map_err(|e| StorageError::Message(format!("wal read error: {e}")))?;
            if n == 0 {
                break;
            }
            if !buf.ends_with(b"\n") {
                break;
            }
            let mut line = &buf[..buf.len() - 1];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            if !line.iter().all(|b| b.is_ascii_whitespace()) {
                let text = std::str::from_utf8(line)
                    .map_err(|e| StorageError::Message(format!("wal decode error: {e}")))?;
                let record: WalRecord = serde_json::from_str(text)
                    .map_err(|e| StorageError::Message(format!("wal decode error: {e}")))?;
                apply(record)?;
            }
            durable_end += n as u64;
        }
        durable_end
    };
    if durable_end < file_len {
        truncate_wal(path, durable_end)?;
    }
    Ok(())
}

fn truncate_wal(path: &std::path::Path, len: u64) -> Result<(), StorageError> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| StorageError::Message(format!("wal open error: {e}")))?;
    file.set_len(len)
        .map_err(|e| StorageError::Message(format!("wal write error: {e}")))?;
    file.sync_data()
        .map_err(|e| StorageError::Message(format!("wal sync error: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HeapEngine, StorageEngine};
    use std::path::PathBuf;

    #[test]
    fn append_and_replay() {
        let dir = std::env::temp_dir().join(format!("rusql-wal-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path: PathBuf = dir.join("rusql.wal");

        let record = WalRecord::CreateTable {
            range_partition: None,
            schema: DEFAULT_SCHEMA.into(),
            name: "t".into(),
            columns: vec![ColumnDef::new("id", "INT")],
            auto_increment_next: None,
            foreign_keys: vec![],
            check_constraints: vec![],
        };
        append_record(&path, &record, true).unwrap();
        append_record(
            &path,
            &WalRecord::Insert {
                table: "t".into(),
                row: vec!["1".into()],
            },
            true,
        )
        .unwrap();

        let mut engine = HeapEngine::new();
        replay_into(&path, &mut |rec| {
            crate::persistent::apply_wal_record(&mut engine, rec)
        })
        .unwrap();

        assert_eq!(engine.scan("t").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_database_old_wal_deserializes_without_charset() {
        let rec: WalRecord =
            serde_json::from_str(r#"{"op":"create_database","name":"app_db"}"#).unwrap();
        match rec {
            WalRecord::CreateDatabase {
                name,
                character_set,
                collation,
            } => {
                assert_eq!(name, "app_db");
                assert_eq!(character_set, None);
                assert_eq!(collation, None);
            }
            other => panic!("expected CreateDatabase, got {other:?}"),
        }
        let with_cs = WalRecord::from_create_database_with_charset(
            "gap_cs",
            Some("utf8mb4"),
            Some("utf8mb4_unicode_ci"),
        );
        let json = serde_json::to_string(&with_cs).unwrap();
        assert!(json.contains("character_set"));
        assert!(json.contains("utf8mb4_unicode_ci"));
        let round: WalRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(round, with_cs);
    }

    fn wal_test_dir(suffix: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rusql-wal-crash-{}-{}", std::process::id(), suffix));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn crash_recovery_torn_tail_keeps_prior_commit() {
        let dir = wal_test_dir("torn");
        let path = dir.join("rusql.wal");
        let record = WalRecord::CreateTable {
            range_partition: None,
            schema: DEFAULT_SCHEMA.into(),
            name: "t".into(),
            columns: vec![ColumnDef::new("id", "INT")],
            auto_increment_next: None,
            foreign_keys: vec![],
            check_constraints: vec![],
        };
        append_record(&path, &record, true).unwrap();
        append_record(
            &path,
            &WalRecord::Insert {
                table: "t".into(),
                row: vec!["1".into()],
            },
            true,
        )
        .unwrap();
        {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            write!(
                file,
                "{{\"op\":\"insert\",\"table\":\"t\",\"row\":[\"torn\"]"
            )
            .unwrap();
            file.flush().unwrap();
        }

        let mut engine = HeapEngine::new();
        replay_into(&path, &mut |rec| {
            crate::persistent::apply_wal_record(&mut engine, rec)
        })
        .unwrap();
        assert_eq!(engine.scan("t").unwrap(), vec![vec!["1".to_string()]]);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("torn"));
        assert!(text.ends_with('\n'));

        append_record(
            &path,
            &WalRecord::Insert {
                table: "t".into(),
                row: vec!["2".into()],
            },
            true,
        )
        .unwrap();
        let mut engine = HeapEngine::new();
        replay_into(&path, &mut |rec| {
            crate::persistent::apply_wal_record(&mut engine, rec)
        })
        .unwrap();
        assert_eq!(
            engine.scan("t").unwrap(),
            vec![vec!["1".to_string()], vec!["2".to_string()]]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_recovery_torn_only_file_opens_empty() {
        let dir = wal_test_dir("torn-only");
        let path = dir.join("rusql.wal");
        std::fs::write(&path, b"{\"op\":\"insert\"").unwrap();
        let mut applied = 0;
        replay_into(&path, &mut |_| {
            applied += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(applied, 0);
        assert_eq!(std::fs::read(&path).unwrap(), b"");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_recovery_corrupt_complete_line_is_not_truncated() {
        let dir = wal_test_dir("corrupt");
        let path = dir.join("rusql.wal");
        append_record(
            &path,
            &WalRecord::Insert {
                table: "t".into(),
                row: vec!["1".into()],
            },
            true,
        )
        .unwrap();
        {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            writeln!(file, "{{\"op\":\"nope\"}}").unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        let err = replay_into(&path, &mut |_| Ok(())).unwrap_err();
        assert!(
            err.to_string().contains("wal decode error"),
            "unexpected error: {err}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

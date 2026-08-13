//! Write-ahead log for rusql persistence (JSON lines).

use rusql_core::{ColumnDef, IndexMeta, TableMeta, DEFAULT_SCHEMA};
use serde::{Deserialize, Serialize};

fn default_schema() -> String {
    DEFAULT_SCHEMA.to_string()
}

use crate::{ColumnAssignment, Row, StorageError};

/// One WAL record (one JSON line in `rusql.wal`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WalRecord {
    CreateDatabase {
        name: String,
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
        column: String,
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
}

impl WalRecord {
    pub fn from_create_database(name: &str) -> Self {
        Self::CreateDatabase {
            name: name.to_string(),
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
        Self::CreateIndex {
            name: meta.name.clone(),
            table: meta.table.clone(),
            column: meta.column.clone(),
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

/// Append one record to the WAL file.
pub fn append_record(path: &std::path::Path, record: &WalRecord) -> Result<(), StorageError> {
    use std::io::Write;
    let line = serde_json::to_string(record)
        .map_err(|e| StorageError::Message(format!("wal encode error: {e}")))?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| StorageError::Message(format!("wal open error: {e}")))?;
    writeln!(file, "{line}").map_err(|e| StorageError::Message(format!("wal write error: {e}")))?;
    file.sync_data()
        .map_err(|e| StorageError::Message(format!("wal sync error: {e}")))?;
    Ok(())
}

/// Replay all records into a heap engine (without re-appending to WAL).
pub fn replay_into(
    path: &std::path::Path,
    apply: &mut dyn FnMut(WalRecord) -> Result<(), StorageError>,
) -> Result<(), StorageError> {
    use std::io::{BufRead, BufReader};
    if !path.exists() {
        return Ok(());
    }
    let reader = BufReader::new(
        std::fs::File::open(path)
            .map_err(|e| StorageError::Message(format!("wal read error: {e}")))?,
    );
    for line in reader.lines() {
        let line = line.map_err(|e| StorageError::Message(format!("wal read error: {e}")))?;
        if line.trim().is_empty() {
            continue;
        }
        let record: WalRecord = serde_json::from_str(&line)
            .map_err(|e| StorageError::Message(format!("wal decode error: {e}")))?;
        apply(record)?;
    }
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
            schema: DEFAULT_SCHEMA.into(),
            name: "t".into(),
            columns: vec![ColumnDef::new("id", "INT")],
            auto_increment_next: None,
        };
        append_record(&path, &record).unwrap();
        append_record(
            &path,
            &WalRecord::Insert {
                table: "t".into(),
                row: vec!["1".into()],
            },
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
}

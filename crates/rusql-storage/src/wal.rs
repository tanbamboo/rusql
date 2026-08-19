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
        replay_into(&path, &mut |rec| match rec {
            WalRecord::CreateDatabase { name } => {
                StorageEngine::create_database(&mut engine, &name)
            }
            WalRecord::DropDatabase { name } => StorageEngine::drop_database(&mut engine, &name),
            WalRecord::CreateTable {
                schema,
                name,
                columns,
            } => engine.create_table(TableMeta {
                name,
                schema,
                columns,
            }),
            WalRecord::Insert { table, row } => engine.insert(&table, row),
            WalRecord::CreateIndex {
                name,
                table,
                column,
            } => engine.create_index(rusql_core::IndexMeta {
                name,
                table,
                column,
            }),
            WalRecord::DropTable { name } => engine.drop_table(&name),
            WalRecord::DeleteRows {
                table,
                column,
                value,
            } => {
                let filter = match (column, value) {
                    (Some(c), Some(v)) => Some(crate::DeleteFilter {
                        column: c,
                        value: v,
                    }),
                    (None, None) => None,
                    _ => return Err(StorageError::Message("invalid DELETE WAL".into())),
                };
                engine.delete_rows(&table, filter).map(|_| ())
            }
            WalRecord::UpdateRows {
                table,
                assignments,
                where_column,
                where_value,
            } => {
                let filter = match (where_column, where_value) {
                    (Some(c), Some(v)) => Some(crate::DeleteFilter {
                        column: c,
                        value: v,
                    }),
                    (None, None) => None,
                    _ => return Err(StorageError::Message("invalid UPDATE WAL".into())),
                };
                engine.update_rows(&table, &assignments, filter).map(|_| ())
            }
            WalRecord::AddColumn { table, column } => engine.add_column(&table, column),
        })
        .unwrap();

        assert_eq!(engine.scan("t").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

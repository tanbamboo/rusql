//! Apply binlog QUERY events and reconstructed INSERT/UPDATE/DELETE row events (MVP replica replay).

use std::path::Path;

use crate::{apply_binlog_bytes, StorageError};

pub fn apply_binlog_file<F>(path: &Path, apply_sql: F) -> Result<u64, StorageError>
where
    F: FnMut(&str, &str) -> Result<(), StorageError>,
{
    let bytes =
        std::fs::read(path).map_err(|e| StorageError::Message(format!("read binlog: {e}")))?;
    apply_binlog_bytes(&bytes, None, apply_sql)
}

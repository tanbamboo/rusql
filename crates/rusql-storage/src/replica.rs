//! Apply binlog QUERY events (MVP replica replay).

use std::path::Path;

use crate::{extract_query_events, strip_gtid_comment, StorageError};

pub fn apply_binlog_file<F>(path: &Path, mut apply_sql: F) -> Result<u64, StorageError>
where
    F: FnMut(&str, &str) -> Result<(), StorageError>,
{
    let bytes =
        std::fs::read(path).map_err(|e| StorageError::Message(format!("read binlog: {e}")))?;
    let events = extract_query_events(&bytes);
    let mut applied = 0u64;
    for sql in events {
        let (sql, _gtid) = strip_gtid_comment(&sql);
        if sql.is_empty() || sql.starts_with("/*") {
            continue;
        }
        apply_sql("rusql", sql)?;
        applied += 1;
    }
    Ok(applied)
}

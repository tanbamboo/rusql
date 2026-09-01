//! MySQL binlog format — production QUERY_EVENT stream (M56) with GTID stub (M58).
//!
//! Checksum algorithm is documented as CRC32 in FORMAT_DESCRIPTION but not appended (MVP).

#![allow(dead_code)]

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::wal::WalRecord;
use crate::StorageError;

/// Binlog file magic (`0xfe` + `bin`).
pub const BINLOG_MAGIC: [u8; 4] = [0xfe, b'b', b'i', b'n'];

const EVENT_HEADER_LEN: usize = 19;
const EVENT_TYPE_QUERY: u8 = 2;
const EVENT_TYPE_FORMAT_DESCRIPTION: u8 = 15;
const BINLOG_VERSION: u16 = 4;
const SERVER_VERSION: &str = "8.0.33-rusql";
const MAX_BINLOG_SIZE: u64 = 1024 * 1024; // 1 MiB rotation (MVP)

/// Post-header lengths for event types 0..=38 (MySQL 8.0 layout).
const POST_HEADER_LEN: [u8; 40] = [
    0, 13, 0, 8, 4, 18, 0, 0, 0, 0, 0, 0, 0, 0, 0, 56, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// GTID state for committed transactions (M58 MVP stub).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GtidState {
    pub server_uuid: String,
    pub sequence: u64,
    pub applied: Vec<String>,
}

impl GtidState {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("gtid.json");
        if !path.exists() {
            return Self {
                server_uuid: uuid_mvp(data_dir),
                ..Default::default()
            };
        }
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> Result<(), StorageError> {
        let path = data_dir.join("gtid.json");
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| StorageError::Message(format!("gtid serialize error: {e}")))?;
        std::fs::write(path, data)
            .map_err(|e| StorageError::Message(format!("gtid write error: {e}")))
    }

    pub fn next_gtid(&mut self) -> String {
        self.sequence += 1;
        format!("{}:{}", self.server_uuid, self.sequence)
    }

    pub fn is_applied(&self, gtid: &str) -> bool {
        self.applied.iter().any(|g| g == gtid)
    }

    pub fn mark_applied(&mut self, gtid: &str) {
        if !self.is_applied(gtid) {
            self.applied.push(gtid.to_string());
        }
    }
}

fn uuid_mvp(data_dir: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    data_dir.display().to_string().hash(&mut h);
    format!("{:016x}-0000-0000-0000-000000000001", h.finish())
}

/// Durable binlog writer with rotation and GTID comment prefix.
#[derive(Debug)]
pub struct BinlogWriter {
    dir: PathBuf,
    server_id: u32,
    current_file: PathBuf,
    position: u32,
    gtid: GtidState,
}

impl BinlogWriter {
    pub fn open(data_dir: &Path, server_id: u32) -> Result<Self, StorageError> {
        let dir = data_dir.join("binlog");
        std::fs::create_dir_all(&dir)
            .map_err(|e| StorageError::Message(format!("binlog dir error: {e}")))?;
        let mut gtid = GtidState::load(data_dir);
        if gtid.server_uuid.is_empty() {
            gtid.server_uuid = uuid_mvp(data_dir);
        }
        let current_file = dir.join("binlog.000001");
        let mut writer = Self {
            dir,
            server_id,
            current_file: current_file.clone(),
            position: 0,
            gtid,
        };
        if !current_file.exists() {
            writer.init_file()?;
        } else {
            writer.position = file_size(&current_file)? as u32;
        }
        Ok(writer)
    }

    fn init_file(&mut self) -> Result<(), StorageError> {
        let mut file = File::create(&self.current_file)
            .map_err(|e| StorageError::Message(format!("binlog create error: {e}")))?;
        file.write_all(&BINLOG_MAGIC)
            .map_err(|e| StorageError::Message(format!("binlog write error: {e}")))?;
        self.position = BINLOG_MAGIC.len() as u32;
        let fde = encode_format_description_event(self.server_id, self.position);
        self.position += fde.len() as u32;
        file.write_all(&fde)
            .map_err(|e| StorageError::Message(format!("binlog write error: {e}")))?;
        Ok(())
    }

    fn maybe_rotate(&mut self) -> Result<(), StorageError> {
        if file_size(&self.current_file)? < MAX_BINLOG_SIZE {
            return Ok(());
        }
        let seq: u32 = self
            .current_file
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.rsplit('.').next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(1)
            + 1;
        self.current_file = self.dir.join(format!("binlog.{seq:06}"));
        self.position = 0;
        self.init_file()
    }

    /// Append QUERY_EVENTs for committed WAL records with GTID comment.
    pub fn append_commit(
        &mut self,
        data_dir: &Path,
        schema: &str,
        records: &[WalRecord],
    ) -> Result<(), StorageError> {
        if records.is_empty() {
            return Ok(());
        }
        self.maybe_rotate()?;
        let gtid = self.gtid.next_gtid();
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.current_file)
            .map_err(|e| StorageError::Message(format!("binlog open error: {e}")))?;
        for record in records {
            if let Some(sql) = wal_record_to_sql(record) {
                let query = format!("/* GTID: {gtid} */ {sql}");
                let qe = encode_query_event(self.server_id, self.position, schema, &query);
                self.position += qe.len() as u32;
                file.write_all(&qe)
                    .map_err(|e| StorageError::Message(format!("binlog write error: {e}")))?;
            }
        }
        self.gtid.save(data_dir)?;
        Ok(())
    }

    pub fn current_path(&self) -> &Path {
        &self.current_file
    }

    pub fn gtid_state(&self) -> &GtidState {
        &self.gtid
    }

    pub fn gtid_state_mut(&mut self) -> &mut GtidState {
        &mut self.gtid
    }
}

/// Write a minimal binlog file: magic + FORMAT_DESCRIPTION_EVENT + QUERY_EVENT (M34 spike).
pub fn write_binlog_spike(
    path: &Path,
    schema: &str,
    query: &str,
    server_id: u32,
) -> Result<(), StorageError> {
    let mut file = File::create(path)
        .map_err(|e| StorageError::Message(format!("binlog create error: {e}")))?;
    file.write_all(&BINLOG_MAGIC)
        .map_err(|e| StorageError::Message(format!("binlog write error: {e}")))?;

    let mut position = BINLOG_MAGIC.len() as u32;
    let fde = encode_format_description_event(server_id, position);
    position += fde.len() as u32;
    file.write_all(&fde)
        .map_err(|e| StorageError::Message(format!("binlog write error: {e}")))?;

    let qe = encode_query_event(server_id, position, schema, query);
    file.write_all(&qe)
        .map_err(|e| StorageError::Message(format!("binlog write error: {e}")))?;
    Ok(())
}

/// Convert a WAL record to SQL text for QUERY_EVENT replication.
pub fn wal_record_to_sql(record: &WalRecord) -> Option<String> {
    match record {
        WalRecord::Insert { table, row } => {
            let vals: Vec<String> = row
                .iter()
                .map(|v| {
                    if v.chars()
                        .all(|c| c.is_ascii_digit() || c == '-' || c == '.')
                    {
                        v.clone()
                    } else {
                        format!("'{v}'")
                    }
                })
                .collect();
            Some(format!("INSERT INTO {table} VALUES ({})", vals.join(", ")))
        }
        WalRecord::UpdateRows {
            table,
            assignments,
            where_column,
            where_value,
        } => {
            let sets: Vec<String> = assignments
                .iter()
                .map(|a| format!("{} = '{}'", a.column, a.value))
                .collect();
            let mut sql = format!("UPDATE {table} SET {}", sets.join(", "));
            if let (Some(col), Some(val)) = (where_column, where_value) {
                sql.push_str(&format!(" WHERE {col} = '{val}'"));
            }
            Some(sql)
        }
        WalRecord::DeleteRows {
            table,
            column,
            value,
        } => {
            let mut sql = format!("DELETE FROM {table}");
            if let (Some(col), Some(val)) = (column, value) {
                sql.push_str(&format!(" WHERE {col} = '{val}'"));
            }
            Some(sql)
        }
        _ => None,
    }
}

fn encode_format_description_event(server_id: u32, position: u32) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&BINLOG_VERSION.to_le_bytes());
    let mut version = [0u8; 50];
    let ver = SERVER_VERSION.as_bytes();
    version[..ver.len().min(50)].copy_from_slice(&ver[..ver.len().min(50)]);
    body.extend_from_slice(&version);
    body.extend_from_slice(&0u32.to_le_bytes());
    body.push(EVENT_HEADER_LEN as u8);
    body.extend_from_slice(&POST_HEADER_LEN);
    body.push(1); // checksum alg: CRC32 (documented; not appended in MVP)

    encode_event(EVENT_TYPE_FORMAT_DESCRIPTION, server_id, position, 0, &body)
}

fn encode_query_event(server_id: u32, position: u32, schema: &str, query: &str) -> Vec<u8> {
    let schema_bytes = schema.as_bytes();
    let query_bytes = query.as_bytes();
    let mut body = Vec::new();
    body.extend_from_slice(&1u32.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.push(schema_bytes.len().try_into().unwrap_or(255));
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(schema_bytes);
    body.push(0);
    body.extend_from_slice(query_bytes);
    encode_event(EVENT_TYPE_QUERY, server_id, position, 0, &body)
}

fn encode_event(event_type: u8, server_id: u32, position: u32, flags: u16, body: &[u8]) -> Vec<u8> {
    let event_length = (EVENT_HEADER_LEN + body.len()) as u32;
    let next_log_pos = position + event_length;
    let mut event = Vec::with_capacity(event_length as usize);
    event.extend_from_slice(&0u32.to_le_bytes());
    event.push(event_type);
    event.extend_from_slice(&server_id.to_le_bytes());
    event.extend_from_slice(&event_length.to_le_bytes());
    event.extend_from_slice(&next_log_pos.to_le_bytes());
    event.extend_from_slice(&flags.to_le_bytes());
    event.extend_from_slice(body);
    event
}

fn file_size(path: &Path) -> Result<u64, StorageError> {
    let meta = std::fs::metadata(path)
        .map_err(|e| StorageError::Message(format!("binlog stat error: {e}")))?;
    Ok(meta.len())
}

/// Return event type byte at `event_offset` (file position after magic).
pub fn event_type_at(data: &[u8], event_offset: usize) -> Option<u8> {
    if event_offset + 4 >= data.len() {
        return None;
    }
    Some(data[event_offset + 4])
}

/// Read all bytes from a binlog file.
pub fn read_binlog_file(path: &Path) -> Result<Vec<u8>, StorageError> {
    let mut file =
        File::open(path).map_err(|e| StorageError::Message(format!("binlog read error: {e}")))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| StorageError::Message(format!("binlog read error: {e}")))?;
    Ok(data)
}

/// Extract QUERY_EVENT SQL payloads from binlog bytes.
pub fn extract_query_events(data: &[u8]) -> Vec<String> {
    let mut queries = Vec::new();
    if data.len() < 4 || data[..4] != BINLOG_MAGIC {
        return queries;
    }
    let mut offset = 4usize;
    while offset + EVENT_HEADER_LEN <= data.len() {
        let event_type = data[offset + 4];
        let event_len =
            u32::from_le_bytes(data[offset + 9..offset + 13].try_into().unwrap()) as usize;
        if event_len < EVENT_HEADER_LEN || offset + event_len > data.len() {
            break;
        }
        if event_type == EVENT_TYPE_QUERY {
            let body = &data[offset + EVENT_HEADER_LEN..offset + event_len];
            if body.len() > 13 {
                let schema_len = body[12] as usize;
                let query_start = 13 + schema_len + 1;
                if query_start <= body.len() {
                    let sql = String::from_utf8_lossy(&body[query_start..]).to_string();
                    queries.push(sql);
                }
            }
        }
        offset += event_len;
    }
    queries
}

/// Strip GTID comment prefix from query text.
pub fn strip_gtid_comment(sql: &str) -> (&str, Option<&str>) {
    let trimmed = sql.trim();
    if let Some(rest) = trimmed.strip_prefix("/* GTID:") {
        if let Some(end) = rest.find("*/") {
            let gtid = rest[..end].trim();
            let sql = rest[end + 2..].trim();
            return (sql, Some(gtid));
        }
    }
    (trimmed, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_magic_format_description_and_query_event() {
        let path = std::env::temp_dir().join(format!(
            "rusql-binlog-spike-{}-{}.bin",
            std::process::id(),
            1u32
        ));
        let _ = std::fs::remove_file(&path);
        write_binlog_spike(&path, "rusql", "INSERT INTO t VALUES (1)", 1).unwrap();

        let bytes = read_binlog_file(&path).unwrap();
        assert_eq!(&bytes[..4], BINLOG_MAGIC);
        assert_eq!(
            event_type_at(&bytes, 4),
            Some(EVENT_TYPE_FORMAT_DESCRIPTION)
        );
        let queries = extract_query_events(&bytes);
        assert!(queries.iter().any(|q| q.contains("INSERT INTO t")));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn binlog_writer_appends_on_commit_with_gtid() {
        let dir = std::env::temp_dir().join(format!("rusql-binlog-writer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut writer = BinlogWriter::open(&dir, 1).unwrap();
        let record = WalRecord::from_insert("t", vec!["1".into()]);
        writer.append_commit(&dir, "rusql", &[record]).unwrap();
        let queries = extract_query_events(&read_binlog_file(writer.current_path()).unwrap());
        assert_eq!(queries.len(), 1);
        assert!(queries[0].contains("GTID:"));
        assert!(queries[0].contains("INSERT INTO t"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wal_record_to_sql_insert_update() {
        let ins =
            wal_record_to_sql(&WalRecord::from_insert("t", vec!["1".into(), "a".into()])).unwrap();
        assert!(ins.contains("INSERT INTO t"));
        let upd = wal_record_to_sql(&WalRecord::from_update(
            "t",
            &[crate::ColumnAssignment {
                column: "v".into(),
                value: "2".into(),
            }],
            Some(&crate::DeleteFilter {
                column: "id".into(),
                value: "1".into(),
            }),
        ))
        .unwrap();
        assert!(upd.contains("UPDATE t SET"));
    }
}

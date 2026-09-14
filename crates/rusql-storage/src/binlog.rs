//! MySQL binlog format — QUERY_EVENT (M56) plus INSERT row events (M72) and GTID stub (M58).
//!
//! INSERT commits emit `TABLE_MAP_EVENT` (19) then `WRITE_ROWS_EVENT_V1` (23). UPDATE/DELETE stay
//! QUERY_EVENT. Row layout is rusql-internal (8-byte table_id, UTF-8 cells); mysqlbinlog is not
//! an oracle. Checksum algorithm is documented as CRC32 in FORMAT_DESCRIPTION but not appended.

#![allow(dead_code)]

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusql_core::table_storage_key;

use crate::wal::WalRecord;
use crate::StorageError;

/// Binlog file magic (`0xfe` + `bin`).
pub const BINLOG_MAGIC: [u8; 4] = [0xfe, b'b', b'i', b'n'];

const EVENT_HEADER_LEN: usize = 19;
const EVENT_TYPE_QUERY: u8 = 2;
const EVENT_TYPE_FORMAT_DESCRIPTION: u8 = 15;
/// `TABLE_MAP_EVENT` (MySQL type 19).
pub const EVENT_TYPE_TABLE_MAP: u8 = 19;
/// `WRITE_ROWS_EVENT_V1` (MySQL type 23).
pub const EVENT_TYPE_WRITE_ROWS_V1: u8 = 23;
const MYSQL_TYPE_VARCHAR: u8 = 0x0f;
const BINLOG_VERSION: u16 = 4;
const SERVER_VERSION: &str = "8.0.33-rusql";
const MAX_BINLOG_SIZE: u64 = 1024 * 1024; // 1 MiB rotation (MVP)

/// Post-header lengths for event types 0..=38 (MySQL 8.0 layout).
/// Slots 19 and 23 are 8 bytes (table_id + flags in MySQL); rusql stores an 8-byte table_id in
/// the event body after the 19-byte common header and does not consume this FDE field when parsing.
const POST_HEADER_LEN: [u8; 40] = [
    0, 13, 0, 8, 4, 18, 0, 0, 0, 0, 0, 0, 0, 0, 0, 56, 0, 0, 0, 8, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0,
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

    /// Append binlog events for committed WAL records.
    ///
    /// INSERT writes `TABLE_MAP` then `WRITE_ROWS_V1`. UPDATE/DELETE remain QUERY_EVENT with a
    /// GTID comment prefix. INSERT with more than 255 cells falls back to QUERY_EVENT.
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
            match record {
                WalRecord::Insert { table, row } if row.len() <= 255 => {
                    let (schema_name, table_name) = split_schema_table(schema, table);
                    let tm = encode_table_map_event(
                        self.server_id,
                        self.position,
                        schema_name,
                        table_name,
                        row.len() as u8,
                    );
                    self.position += tm.len() as u32;
                    file.write_all(&tm)
                        .map_err(|e| StorageError::Message(format!("binlog write error: {e}")))?;
                    let wr = encode_write_rows_event(
                        self.server_id,
                        self.position,
                        schema_name,
                        table_name,
                        row,
                    );
                    self.position += wr.len() as u32;
                    file.write_all(&wr)
                        .map_err(|e| StorageError::Message(format!("binlog write error: {e}")))?;
                }
                _ => {
                    if let Some(sql) = wal_record_to_sql(record) {
                        let query = format!("/* GTID: {gtid} */ {sql}");
                        let qe = encode_query_event(self.server_id, self.position, schema, &query);
                        self.position += qe.len() as u32;
                        file.write_all(&qe).map_err(|e| {
                            StorageError::Message(format!("binlog write error: {e}"))
                        })?;
                    }
                }
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
        WalRecord::Insert { table, row } => Some(insert_sql(table, row)),
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

fn split_schema_table<'a>(default_schema: &'a str, table: &'a str) -> (&'a str, &'a str) {
    if let Some((schema, name)) = table.split_once('.') {
        (schema, name)
    } else {
        (default_schema, table)
    }
}

fn table_id_for(schema: &str, table: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    schema.hash(&mut hasher);
    table.hash(&mut hasher);
    hasher.finish()
}

fn sql_literal(value: &str) -> String {
    if value.is_empty() {
        "NULL".to_string()
    } else if value
        .chars()
        .all(|c| c.is_ascii_digit() || c == '-' || c == '.')
    {
        value.to_string()
    } else {
        format!("'{value}'")
    }
}

fn insert_sql(table: &str, row: &[String]) -> String {
    let vals: Vec<String> = row.iter().map(|v| sql_literal(v)).collect();
    format!("INSERT INTO {table} VALUES ({})", vals.join(", "))
}

fn bitmap_len(width: usize) -> usize {
    width.div_ceil(8)
}

fn set_bit(bits: &mut [u8], index: usize) {
    bits[index / 8] |= 1 << (index % 8);
}

fn bit_is_set(bits: &[u8], index: usize) -> bool {
    bits.get(index / 8)
        .map(|byte| byte & (1 << (index % 8)) != 0)
        .unwrap_or(false)
}

fn encode_table_map_event(
    server_id: u32,
    position: u32,
    schema: &str,
    table: &str,
    col_count: u8,
) -> Vec<u8> {
    let schema_bytes = schema.as_bytes();
    let table_bytes = table.as_bytes();
    let mut body = Vec::new();
    body.extend_from_slice(&table_id_for(schema, table).to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    body.push(schema_bytes.len().try_into().unwrap_or(255));
    body.extend_from_slice(&schema_bytes[..schema_bytes.len().min(255)]);
    body.push(0);
    body.push(table_bytes.len().try_into().unwrap_or(255));
    body.extend_from_slice(&table_bytes[..table_bytes.len().min(255)]);
    body.push(0);
    body.push(col_count);
    body.extend(std::iter::repeat(MYSQL_TYPE_VARCHAR).take(col_count as usize));
    encode_event(EVENT_TYPE_TABLE_MAP, server_id, position, 0, &body)
}

fn encode_write_rows_event(
    server_id: u32,
    position: u32,
    schema: &str,
    table: &str,
    row: &[String],
) -> Vec<u8> {
    let width = row.len();
    let mut body = Vec::new();
    body.extend_from_slice(&table_id_for(schema, table).to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    body.push(width as u8);
    let mut included = vec![0u8; bitmap_len(width)];
    for i in 0..width {
        set_bit(&mut included, i);
    }
    body.extend_from_slice(&included);
    let mut nulls = vec![0u8; bitmap_len(width)];
    for (i, cell) in row.iter().enumerate() {
        if cell.is_empty() {
            set_bit(&mut nulls, i);
        }
    }
    body.extend_from_slice(&nulls);
    for cell in row {
        if cell.is_empty() {
            continue;
        }
        let bytes = cell.as_bytes();
        body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        body.extend_from_slice(bytes);
    }
    encode_event(EVENT_TYPE_WRITE_ROWS_V1, server_id, position, 0, &body)
}

fn parse_table_map(body: &[u8]) -> Option<(u64, String, String)> {
    if body.len() < 11 {
        return None;
    }
    let table_id = u64::from_le_bytes(body[0..8].try_into().ok()?);
    let mut offset = 10;
    let schema_len = *body.get(offset)? as usize;
    offset += 1;
    if offset + schema_len + 1 > body.len() {
        return None;
    }
    let schema = String::from_utf8_lossy(&body[offset..offset + schema_len]).into_owned();
    offset += schema_len;
    if body[offset] != 0 {
        return None;
    }
    offset += 1;
    let table_len = *body.get(offset)? as usize;
    offset += 1;
    if offset + table_len + 1 > body.len() {
        return None;
    }
    let table = String::from_utf8_lossy(&body[offset..offset + table_len]).into_owned();
    offset += table_len;
    if body.get(offset).copied() != Some(0) {
        return None;
    }
    Some((table_id, schema, table))
}

fn parse_write_rows(body: &[u8]) -> Option<(u64, Vec<String>)> {
    if body.len() < 11 {
        return None;
    }
    let table_id = u64::from_le_bytes(body[0..8].try_into().ok()?);
    let width = *body.get(10)? as usize;
    let mut offset = 11;
    let incl_len = bitmap_len(width);
    if offset + incl_len > body.len() {
        return None;
    }
    let included = &body[offset..offset + incl_len];
    offset += incl_len;
    let null_len = bitmap_len(width);
    if offset + null_len > body.len() {
        return None;
    }
    let nulls = &body[offset..offset + null_len];
    offset += null_len;
    let mut row = Vec::with_capacity(width);
    for i in 0..width {
        if !bit_is_set(included, i) || bit_is_set(nulls, i) {
            row.push(String::new());
            continue;
        }
        if offset + 4 > body.len() {
            return None;
        }
        let len = u32::from_le_bytes(body[offset..offset + 4].try_into().ok()?) as usize;
        offset += 4;
        if offset + len > body.len() {
            return None;
        }
        row.push(String::from_utf8_lossy(&body[offset..offset + len]).into_owned());
        offset += len;
    }
    Some((table_id, row))
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

/// Extract QUERY_EVENT SQL and INSERT statements reconstructed from TABLE_MAP + WRITE_ROWS.
pub fn extract_query_events(data: &[u8]) -> Vec<String> {
    let mut queries = Vec::new();
    let mut table_maps: HashMap<u64, (String, String)> = HashMap::new();
    for event in events_from_position(data, 4) {
        if event.len() < EVENT_HEADER_LEN {
            continue;
        }
        let body = &event[EVENT_HEADER_LEN..];
        match event[4] {
            EVENT_TYPE_QUERY => {
                if body.len() > 13 {
                    let schema_len = body[12] as usize;
                    let query_start = 13 + schema_len + 1;
                    if query_start <= body.len() {
                        let sql = String::from_utf8_lossy(&body[query_start..]).to_string();
                        queries.push(sql);
                    }
                }
            }
            EVENT_TYPE_TABLE_MAP => {
                if let Some((table_id, schema, table)) = parse_table_map(body) {
                    table_maps.insert(table_id, (schema, table));
                }
            }
            EVENT_TYPE_WRITE_ROWS_V1 => {
                if let Some((table_id, row)) = parse_write_rows(body) {
                    if let Some((schema, table)) = table_maps.get(&table_id) {
                        let storage_key = table_storage_key(schema, table);
                        queries.push(insert_sql(&storage_key, &row));
                    }
                }
            }
            _ => {}
        }
    }
    queries
}

/// Event slices at or after `position` (file offset). Magic is never included.
/// Position `0` is treated as the first event (offset 4).
pub fn events_from_position(data: &[u8], position: u32) -> Vec<&[u8]> {
    let mut events = Vec::new();
    if data.len() < EVENT_HEADER_LEN {
        return events;
    }
    let mut offset = if data.len() >= 4 && data[..4] == BINLOG_MAGIC {
        4usize
    } else {
        0usize
    };
    let start = if position <= 4 {
        offset
    } else {
        position as usize
    };
    while offset + EVENT_HEADER_LEN <= data.len() {
        let event_len =
            u32::from_le_bytes(data[offset + 9..offset + 13].try_into().unwrap()) as usize;
        if event_len < EVENT_HEADER_LEN || offset + event_len > data.len() {
            break;
        }
        if offset >= start {
            events.push(&data[offset..offset + event_len]);
        }
        offset += event_len;
    }
    events
}

/// Replication dump payloads: `0x00` + event bytes, from `position`.
pub fn dump_event_packets(data: &[u8], position: u32) -> Vec<Vec<u8>> {
    events_from_position(data, position)
        .into_iter()
        .map(|event| {
            let mut packet = Vec::with_capacity(1 + event.len());
            packet.push(0x00);
            packet.extend_from_slice(event);
            packet
        })
        .collect()
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
        let from_start = events_from_position(&bytes, 4);
        assert!(!from_start.is_empty());
        assert_eq!(from_start[0][4], EVENT_TYPE_FORMAT_DESCRIPTION);
        let packets = dump_event_packets(&bytes, 4);
        assert_eq!(packets.len(), from_start.len());
        assert_eq!(packets[0][0], 0x00);
        assert_eq!(&packets[0][1..], from_start[0]);
        let after_fde = 4 + from_start[0].len();
        let rest = events_from_position(&bytes, after_fde as u32);
        assert!(rest.iter().any(|e| e[4] == EVENT_TYPE_QUERY));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn binlog_writer_insert_emits_table_map_and_write_rows() {
        let dir =
            std::env::temp_dir().join(format!("rusql-binlog-writer-insert-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut writer = BinlogWriter::open(&dir, 1).unwrap();
        let record = WalRecord::from_insert("t", vec!["1".into(), "a".into(), "".into()]);
        writer.append_commit(&dir, "rusql", &[record]).unwrap();
        let bytes = read_binlog_file(writer.current_path()).unwrap();
        let events = events_from_position(&bytes, 4);
        assert_eq!(events[0][4], EVENT_TYPE_FORMAT_DESCRIPTION);
        assert!(events.iter().any(|e| e[4] == EVENT_TYPE_TABLE_MAP));
        assert!(events.iter().any(|e| e[4] == EVENT_TYPE_WRITE_ROWS_V1));
        assert!(events.iter().all(|e| e[4] != EVENT_TYPE_QUERY));
        let packets = dump_event_packets(&bytes, 4);
        assert!(packets
            .iter()
            .any(|p| p.get(5) == Some(&EVENT_TYPE_TABLE_MAP)));
        assert!(packets
            .iter()
            .any(|p| p.get(5) == Some(&EVENT_TYPE_WRITE_ROWS_V1)));
        let queries = extract_query_events(&bytes);
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0], "INSERT INTO t VALUES (1, 'a', NULL)");
        assert!(!queries[0].contains("GTID:"));
        assert_eq!(writer.gtid_state().sequence, 1);
        let applied = crate::apply_binlog_file(writer.current_path(), |_schema, sql| {
            assert_eq!(sql, "INSERT INTO t VALUES (1, 'a', NULL)");
            Ok(())
        })
        .unwrap();
        assert_eq!(applied, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binlog_writer_update_still_query_event_with_gtid() {
        let dir =
            std::env::temp_dir().join(format!("rusql-binlog-writer-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut writer = BinlogWriter::open(&dir, 1).unwrap();
        let record = WalRecord::from_update(
            "t",
            &[crate::ColumnAssignment {
                column: "v".into(),
                value: "2".into(),
            }],
            Some(&crate::DeleteFilter {
                column: "id".into(),
                value: "1".into(),
            }),
        );
        writer.append_commit(&dir, "rusql", &[record]).unwrap();
        let bytes = read_binlog_file(writer.current_path()).unwrap();
        let events = events_from_position(&bytes, 4);
        assert!(events.iter().any(|e| e[4] == EVENT_TYPE_QUERY));
        assert!(events.iter().all(|e| e[4] != EVENT_TYPE_WRITE_ROWS_V1));
        let queries = extract_query_events(&bytes);
        assert_eq!(queries.len(), 1);
        assert!(queries[0].contains("GTID:"));
        assert!(queries[0].contains("UPDATE t SET"));
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

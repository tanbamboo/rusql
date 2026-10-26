//! MySQL binlog format — QUERY_EVENT (M56), INSERT row events (M72), UPDATE/DELETE row events (M74),
//! row event v2 / partial images (M180).
//!
//! INSERT commits emit `GTID_LOG_EVENT` (33) then `TABLE_MAP_EVENT` (19) then
//! `WRITE_ROWS_EVENT` v2 (30). UPDATE/DELETE emit `GTID` then `TABLE_MAP` then
//! `UPDATE_ROWS_EVENT` v2 (31) / `DELETE_ROWS_EVENT` v2 (32). v1 types 23–25 still apply.
//! Row layout is rusql-internal (8-byte table_id, UTF-8 cells) plus the MySQL v2 extra-data
//! length. mysqlbinlog is not an oracle. Checksum algorithm is documented as CRC32 in
//! FORMAT_DESCRIPTION but not appended.

#![allow(dead_code)]

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusql_core::table_storage_key;

use crate::wal::WalRecord;
use crate::{ColumnAssignment, StorageError};

/// Binlog file magic (`0xfe` + `bin`).
pub const BINLOG_MAGIC: [u8; 4] = [0xfe, b'b', b'i', b'n'];

const EVENT_HEADER_LEN: usize = 19;
const EVENT_TYPE_QUERY: u8 = 2;
const EVENT_TYPE_FORMAT_DESCRIPTION: u8 = 15;
/// `TABLE_MAP_EVENT` (MySQL type 19).
pub const EVENT_TYPE_TABLE_MAP: u8 = 19;
/// `WRITE_ROWS_EVENT_V1` (MySQL type 23).
pub const EVENT_TYPE_WRITE_ROWS_V1: u8 = 23;
/// `UPDATE_ROWS_EVENT_V1` (MySQL type 24).
pub const EVENT_TYPE_UPDATE_ROWS_V1: u8 = 24;
/// `DELETE_ROWS_EVENT_V1` (MySQL type 25).
pub const EVENT_TYPE_DELETE_ROWS_V1: u8 = 25;
/// `WRITE_ROWS_EVENT` v2 (MySQL type 30).
pub const EVENT_TYPE_WRITE_ROWS_V2: u8 = 30;
/// `UPDATE_ROWS_EVENT` v2 (MySQL type 31).
pub const EVENT_TYPE_UPDATE_ROWS_V2: u8 = 31;
/// `DELETE_ROWS_EVENT` v2 (MySQL type 32).
pub const EVENT_TYPE_DELETE_ROWS_V2: u8 = 32;
/// `ER_SLAVE_CORRUPT_EVENT` — v2 row header cannot be applied.
pub const ER_SLAVE_CORRUPT_EVENT: u16 = 1610;
/// `GTID_LOG_EVENT` (MySQL type 33).
pub const EVENT_TYPE_GTID_LOG: u8 = 33;
/// `HEARTBEAT_LOG_EVENT` (MySQL type 27) — synthetic on idle dump connections (M174).
pub const EVENT_TYPE_HEARTBEAT: u8 = 27;
/// Default `master_heartbeat_period` when the replica omits it in `COM_BINLOG_DUMP` (seconds).
pub const DEFAULT_MASTER_HEARTBEAT_PERIOD_SECS: u64 = 30;
const MYSQL_TYPE_VARCHAR: u8 = 0x0f;
const GTID_LOGICAL_TIMESTAMP_TYPECODE: u8 = 2;
/// MySQL 8.0.33 wire version field in `GTID_LOG_EVENT` (80033).
const GTID_IMMEDIATE_SERVER_VERSION: u32 = 80033;
const BINLOG_VERSION: u16 = 4;
const SERVER_VERSION: &str = "8.0.33-rusql";
const MAX_BINLOG_SIZE: u64 = 1024 * 1024; // 1 MiB rotation (MVP)

/// Post-header lengths for event types 0..=39 (MySQL 8.0 layout).
/// Slots 19 and 23..=25 are 8 bytes (v1 table_id + flags). Slots 30..=32 are 10 bytes (v2 adds the
/// 2-byte extra-data length). rusql stores an 8-byte table_id in the event body after the 19-byte
/// common header and does not consume this FDE field when parsing. TABLE_MAP column types stay
/// `MYSQL_TYPE_VARCHAR`.
const POST_HEADER_LEN: [u8; 40] = [
    0, 13, 0, 8, 4, 18, 0, 0, 0, 0, 0, 0, 0, 0, 0, 56, 0, 0, 0, 8, 0, 0, 0, 8, 8, 8, 0, 0, 0, 0,
    10, 10, 10, 19, 0, 0, 0, 0, 0, 0,
];

/// MySQL `binlog_row_image`. Default is `FULL` (MySQL 8.0).
///
/// `MINIMAL` logs only the identifying columns in the before image and only changed columns in the
/// after image. `NOBLOB` logs every column except unchanged BLOB/TEXT columns. `FULL` logs every
/// column the encoder was given. `INSERT` / `WRITE_ROWS` always logs the full row for every mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinlogRowImage {
    Full,
    Minimal,
    Noblob,
}

impl BinlogRowImage {
    /// MySQL variable text (`FULL`, `MINIMAL`, `NOBLOB`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "FULL",
            Self::Minimal => "MINIMAL",
            Self::Noblob => "NOBLOB",
        }
    }

    fn tag(self) -> u8 {
        match self {
            Self::Full => 0,
            Self::Minimal => 1,
            Self::Noblob => 2,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Full),
            1 => Some(Self::Minimal),
            2 => Some(Self::Noblob),
            _ => None,
        }
    }
}

/// Default `binlog_row_image` when `{data_dir}/binlog-row-image.json` is absent.
pub const DEFAULT_BINLOG_ROW_IMAGE: BinlogRowImage = BinlogRowImage::Full;

const ROW_IMAGE_FILE: &str = "binlog-row-image.json";
const IMAGE_FLAG_BLOB: u8 = 0x01;
const IMAGE_FLAG_PK: u8 = 0x02;

/// One column in a before or after row image. Empty `value` is SQL NULL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageColumn {
    pub name: String,
    pub value: String,
    pub is_blob: bool,
    pub primary_key: bool,
}

/// Client-visible failure for `binlog_row_image` or a corrupt v2 row event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowEventError {
    pub code: u16,
    pub message: String,
}

/// Parsed v2 `UPDATE_ROWS` image (columns already reduced by `binlog_row_image`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedUpdateRows {
    pub table_id: u64,
    pub image: BinlogRowImage,
    pub before: Vec<ImageColumn>,
    pub after: Vec<ImageColumn>,
}

/// Parse MySQL `binlog_row_image` text. Unknown text is errno 1231.
pub fn parse_binlog_row_image(value: &str) -> Result<BinlogRowImage, RowEventError> {
    match value.trim().to_ascii_uppercase().as_str() {
        "FULL" => Ok(BinlogRowImage::Full),
        "MINIMAL" => Ok(BinlogRowImage::Minimal),
        "NOBLOB" => Ok(BinlogRowImage::Noblob),
        _ => Err(RowEventError {
            code: crate::ER_WRONG_VALUE_FOR_VAR,
            message: rusql_i18n::messages::sql_wrong_value_for_var("binlog_row_image", value),
        }),
    }
}

fn load_row_image(data_dir: &Path) -> BinlogRowImage {
    let path = data_dir.join(ROW_IMAGE_FILE);
    let Ok(text) = std::fs::read_to_string(path) else {
        return DEFAULT_BINLOG_ROW_IMAGE;
    };
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get("image").and_then(|s| s.as_str()).map(str::to_string))
        .and_then(|s| parse_binlog_row_image(&s).ok())
        .unwrap_or(DEFAULT_BINLOG_ROW_IMAGE)
}

/// `SET [GLOBAL|SESSION] binlog_row_image = …`. `Ok(None)` when `stmt` is a different statement.
pub fn binlog_row_image_assignment(
    stmt: &sqlparser::ast::Statement,
) -> Result<Option<BinlogRowImage>, RowEventError> {
    let sqlparser::ast::Statement::SetVariable {
        variables, value, ..
    } = stmt
    else {
        return Ok(None);
    };
    let vars: &[sqlparser::ast::ObjectName] = variables.as_ref();
    if vars.len() != 1 || value.len() != 1 {
        return Ok(None);
    }
    if set_var_name(&vars[0]) != "binlog_row_image" {
        return Ok(None);
    }
    let raw = set_expr_text(&value[0]);
    parse_binlog_row_image(&raw).map(Some)
}

fn set_var_name(name: &sqlparser::ast::ObjectName) -> String {
    let parts: Vec<&str> = name.0.iter().map(|p| p.value.as_str()).collect();
    let Some(first) = parts.first().copied() else {
        return String::new();
    };
    if let Some(rest) = first.strip_prefix("@@") {
        if rest.is_empty() {
            return String::new();
        }
        if rest.eq_ignore_ascii_case("global")
            || rest.eq_ignore_ascii_case("session")
            || rest.eq_ignore_ascii_case("local")
        {
            return parts.get(1).copied().unwrap_or("").to_ascii_lowercase();
        }
        return rest.to_ascii_lowercase();
    }
    if parts.len() >= 2
        && matches!(
            first.to_ascii_lowercase().as_str(),
            "global" | "session" | "local"
        )
    {
        return parts[1].to_ascii_lowercase();
    }
    if first.starts_with('@') {
        return String::new();
    }
    parts.join(".").to_ascii_lowercase()
}

fn set_expr_text(expr: &sqlparser::ast::Expr) -> String {
    match expr {
        sqlparser::ast::Expr::Value(sqlparser::ast::Value::SingleQuotedString(s))
        | sqlparser::ast::Expr::Value(sqlparser::ast::Value::DoubleQuotedString(s)) => s.clone(),
        sqlparser::ast::Expr::Value(sqlparser::ast::Value::Number(n, _)) => n.clone(),
        sqlparser::ast::Expr::Identifier(id) => id.value.clone(),
        other => other.to_string(),
    }
}

/// Columns to log for an UPDATE under `image`.
///
/// `before_row` is the pre-image (old values). `after_row` is the post-image (new values).
/// Missing columns are not invented: the caller passes the columns it knows.
pub fn partial_update_images(
    before_row: &[ImageColumn],
    after_row: &[ImageColumn],
    image: BinlogRowImage,
) -> (Vec<ImageColumn>, Vec<ImageColumn>) {
    match image {
        BinlogRowImage::Full => (before_row.to_vec(), after_row.to_vec()),
        BinlogRowImage::Minimal => {
            let any_pk =
                before_row.iter().any(|c| c.primary_key) || after_row.iter().any(|c| c.primary_key);
            let before: Vec<ImageColumn> = before_row
                .iter()
                .filter(|c| if any_pk { c.primary_key } else { true })
                .cloned()
                .collect();
            let after: Vec<ImageColumn> = after_row
                .iter()
                .filter(|c| column_changed(before_row, c))
                .cloned()
                .collect();
            (before, after)
        }
        BinlogRowImage::Noblob => {
            let before: Vec<ImageColumn> = before_row
                .iter()
                .filter(|c| noblob_keep(before_row, after_row, c))
                .cloned()
                .collect();
            let after: Vec<ImageColumn> = after_row
                .iter()
                .filter(|c| noblob_keep(before_row, after_row, c))
                .cloned()
                .collect();
            (before, after)
        }
    }
}

fn column_changed(before_row: &[ImageColumn], after: &ImageColumn) -> bool {
    match before_row.iter().find(|c| c.name == after.name) {
        Some(old) => old.value != after.value,
        None => true,
    }
}

fn noblob_keep(before_row: &[ImageColumn], after_row: &[ImageColumn], col: &ImageColumn) -> bool {
    if !col.is_blob || col.primary_key {
        return true;
    }
    let old = before_row.iter().find(|c| c.name == col.name);
    let new = after_row.iter().find(|c| c.name == col.name);
    match (old, new) {
        (Some(old), Some(new)) => old.value != new.value,
        _ => true,
    }
}

/// Apply a partial after-image onto `row`. Columns absent from `after` stay as they are.
///
/// Returns false when a before-image column is missing or does not match (the event is not for
/// this row). Does not insert NULL for omitted columns.
pub fn apply_partial_update(
    row: &mut std::collections::BTreeMap<String, String>,
    before: &[ImageColumn],
    after: &[ImageColumn],
) -> bool {
    for col in before {
        match row.get(&col.name) {
            Some(value) if value == &col.value => {}
            _ => return false,
        }
    }
    for col in after {
        row.insert(col.name.clone(), col.value.clone());
    }
    true
}

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
    row_image: BinlogRowImage,
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
            row_image: load_row_image(data_dir),
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
    /// INSERT writes `TABLE_MAP` then `WRITE_ROWS` v2 (full row for every `binlog_row_image`).
    /// UPDATE/DELETE write `TABLE_MAP` then `UPDATE_ROWS` / `DELETE_ROWS` v2 using `row_image`.
    /// Other records remain QUERY_EVENT. INSERT with more than 255 cells falls back to QUERY_EVENT.
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
        let dml_bytes: u64 =
            encode_wal_records_as_events(self.server_id, schema, records, 0, self.row_image)?
                .iter()
                .map(|e| e.len() as u64)
                .sum();
        let gtid_len_estimate = gtid_log_event_wire_length(dml_bytes);
        let tx_length = gtid_len_estimate as u64 + dml_bytes;
        let gtid_event = encode_gtid_log_event(self.server_id, self.position, &gtid, tx_length)?;
        let gtid_len = gtid_event.len();
        let dml_events = encode_wal_records_as_events(
            self.server_id,
            schema,
            records,
            self.position + gtid_len as u32,
            self.row_image,
        )?;
        self.position += gtid_len as u32;
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.current_file)
            .map_err(|e| StorageError::Message(format!("binlog open error: {e}")))?;
        file.write_all(&gtid_event)
            .map_err(|e| StorageError::Message(format!("binlog write error: {e}")))?;
        for event in &dml_events {
            self.position += event.len() as u32;
            file.write_all(event)
                .map_err(|e| StorageError::Message(format!("binlog write error: {e}")))?;
        }
        self.gtid.save(data_dir)?;
        Ok(())
    }

    pub fn current_path(&self) -> &Path {
        &self.current_file
    }

    pub fn server_id(&self) -> u32 {
        self.server_id
    }

    pub fn current_position(&self) -> u32 {
        self.position
    }

    pub fn gtid_state(&self) -> &GtidState {
        &self.gtid
    }

    pub fn gtid_state_mut(&mut self) -> &mut GtidState {
        &mut self.gtid
    }

    /// Effective `binlog_row_image` for the next commit (process-wide, persisted).
    pub fn row_image(&self) -> BinlogRowImage {
        self.row_image
    }

    /// Persist `binlog_row_image` in `{data_dir}/binlog-row-image.json` and use it for later commits.
    pub fn set_row_image(
        &mut self,
        data_dir: &Path,
        image: BinlogRowImage,
    ) -> Result<(), StorageError> {
        let path = data_dir.join(ROW_IMAGE_FILE);
        let body = serde_json::json!({ "image": image.as_str() });
        std::fs::write(&path, body.to_string()).map_err(|_| {
            StorageError::Message(rusql_i18n::messages::sql_binlog_row_image_persist_failed())
        })?;
        self.row_image = image;
        Ok(())
    }
}

/// One row of `SHOW BINARY LOGS` (`Log_name`, `File_size`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryLogFile {
    pub name: String,
    pub size: u64,
}

/// List `binlog.NNNNNN` files under `{data_dir}/binlog`.
///
/// Missing directory or no matching files yields an empty list (documented
/// `SHOW BINARY LOGS` result when the binlog file is absent).
pub fn list_binary_logs(data_dir: &Path) -> Vec<BinaryLogFile> {
    let dir = data_dir.join("binlog");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut logs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !is_binlog_filename(name) {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        logs.push(BinaryLogFile {
            name: name.to_string(),
            size,
        });
    }
    logs.sort_by(|a, b| a.name.cmp(&b.name));
    logs
}

/// Live `SHOW MASTER STATUS` row: current binlog file and end offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MasterBinlogStatus {
    pub file: String,
    pub position: u32,
}

/// Read the active binlog file and end log position from disk.
///
/// Uses the lexicographically last `binlog.NNNNNN` file (current after rotation).
/// Missing directory or unreadable file yields `None` (documented empty result).
pub fn master_binlog_status(data_dir: &Path) -> Option<MasterBinlogStatus> {
    let logs = list_binary_logs(data_dir);
    let last = logs.last()?;
    let path = data_dir.join("binlog").join(&last.name);
    let data = read_binlog_file(&path).ok()?;
    let (_, position) = dump_events_with_next_position(&data, 4);
    Some(MasterBinlogStatus {
        file: last.name.clone(),
        position,
    })
}

fn is_binlog_filename(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("binlog.") else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

/// One row of `SHOW BINLOG EVENTS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinlogEventRow {
    pub log_name: String,
    pub pos: u32,
    pub event_type: String,
    pub server_id: u32,
    pub end_log_pos: u32,
    pub info: String,
}

/// Read events from a known `binlog.NNNNNN` file under `{data_dir}/binlog`.
///
/// `log_name` omitted uses the first listed file (MySQL default). Missing
/// directory, missing file, or unreadable bytes yield an empty list. Truncated
/// tails stop at the last complete event (no crash).
pub fn list_binlog_events(
    data_dir: &Path,
    log_name: Option<&str>,
    from_pos: Option<u32>,
) -> Vec<BinlogEventRow> {
    let logs = list_binary_logs(data_dir);
    let name = match log_name {
        Some(n) => n.to_string(),
        None => match logs.first() {
            Some(f) => f.name.clone(),
            None => return Vec::new(),
        },
    };
    let path = data_dir.join("binlog").join(&name);
    let Ok(data) = read_binlog_file(&path) else {
        return Vec::new();
    };
    binlog_event_rows(&data, &name, from_pos.unwrap_or(4))
}

/// Parse complete events from binlog bytes into `SHOW BINLOG EVENTS` rows.
pub fn binlog_event_rows(data: &[u8], log_name: &str, from_pos: u32) -> Vec<BinlogEventRow> {
    let mut rows = Vec::new();
    if data.len() < EVENT_HEADER_LEN {
        return rows;
    }
    let mut offset = if data.len() >= 4 && data[..4] == BINLOG_MAGIC {
        4usize
    } else {
        0usize
    };
    let start = if from_pos <= 4 {
        offset
    } else {
        from_pos as usize
    };
    while offset + EVENT_HEADER_LEN <= data.len() {
        let event_len =
            u32::from_le_bytes(data[offset + 9..offset + 13].try_into().unwrap()) as usize;
        if event_len < EVENT_HEADER_LEN || offset + event_len > data.len() {
            break;
        }
        if offset >= start {
            rows.push(event_to_show_row(
                log_name,
                offset as u32,
                &data[offset..offset + event_len],
            ));
        }
        offset += event_len;
    }
    rows
}

fn event_to_show_row(log_name: &str, pos: u32, event: &[u8]) -> BinlogEventRow {
    let event_type = event[4];
    let server_id = u32::from_le_bytes(event[5..9].try_into().unwrap_or([0; 4]));
    let end_log_pos = u32::from_le_bytes(event[13..17].try_into().unwrap_or([0; 4]));
    BinlogEventRow {
        log_name: log_name.to_string(),
        pos,
        event_type: event_type_name(event_type),
        server_id,
        end_log_pos,
        info: event_info(event),
    }
}

/// MySQL `SHOW BINLOG EVENTS` `Event_type` names; unknown types use `"Unknown"`.
pub fn event_type_name(event_type: u8) -> String {
    match event_type {
        1 => "Start",
        2 => "Query",
        3 => "Stop",
        4 => "Rotate",
        5 => "Intvar",
        13 => "Rand",
        14 => "User var",
        15 => "Format_desc",
        16 => "Xid",
        19 => "Table_map",
        23 | 30 => "Write_rows",
        24 | 31 => "Update_rows",
        25 | 32 => "Delete_rows",
        27 => "Heartbeat",
        29 => "Rows_query",
        33 => "Gtid",
        34 => "Anonymous_Gtid",
        35 => "Previous_gtids",
        _ => "Unknown",
    }
    .to_string()
}

/// Pretty-print Info for rusql-written event types; unknown types get an empty stub.
fn event_info(event: &[u8]) -> String {
    if event.len() < EVENT_HEADER_LEN {
        return String::new();
    }
    let body = &event[EVENT_HEADER_LEN..];
    match event[4] {
        EVENT_TYPE_FORMAT_DESCRIPTION => format_desc_info(body),
        EVENT_TYPE_QUERY => query_event_sql(body).unwrap_or_default(),
        EVENT_TYPE_TABLE_MAP => parse_table_map(body)
            .map(|(id, schema, table)| format!("table_id: {id} ({schema}.{table})"))
            .unwrap_or_default(),
        EVENT_TYPE_WRITE_ROWS_V1
        | EVENT_TYPE_UPDATE_ROWS_V1
        | EVENT_TYPE_DELETE_ROWS_V1
        | EVENT_TYPE_WRITE_ROWS_V2
        | EVENT_TYPE_UPDATE_ROWS_V2
        | EVENT_TYPE_DELETE_ROWS_V2 => {
            if body.len() >= 8 {
                let table_id = u64::from_le_bytes(body[0..8].try_into().unwrap_or([0; 8]));
                format!("table_id: {table_id}")
            } else {
                String::new()
            }
        }
        EVENT_TYPE_GTID_LOG => parse_gtid_log_event(body)
            .map(|g| format!("SET @@SESSION.GTID_NEXT= '{g}'"))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn format_desc_info(body: &[u8]) -> String {
    if body.len() < 52 {
        return String::new();
    }
    let ver = u16::from_le_bytes(body[0..2].try_into().unwrap_or([0; 2]));
    let server_bytes = &body[2..52];
    let end = server_bytes.iter().position(|&b| b == 0).unwrap_or(50);
    let server = String::from_utf8_lossy(&server_bytes[..end]);
    format!("Server ver: {server}, Binlog ver: {ver}")
}

fn query_event_sql(body: &[u8]) -> Option<String> {
    if body.len() <= 13 {
        return None;
    }
    let schema_len = body[12] as usize;
    let query_start = 13 + schema_len + 1;
    if query_start > body.len() {
        return None;
    }
    Some(String::from_utf8_lossy(&body[query_start..]).into_owned())
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

fn encode_wal_records_as_events(
    server_id: u32,
    schema: &str,
    records: &[WalRecord],
    start_position: u32,
    image: BinlogRowImage,
) -> Result<Vec<Vec<u8>>, StorageError> {
    let mut events = Vec::new();
    let mut position = start_position;
    for record in records {
        let event = match record {
            WalRecord::Insert { table, row } if row.len() <= 255 => {
                let (schema_name, table_name) = split_schema_table(schema, table);
                let tm = encode_table_map_event(
                    server_id,
                    position,
                    schema_name,
                    table_name,
                    row.len() as u8,
                );
                position += tm.len() as u32;
                let wr = encode_write_rows_event(server_id, position, schema_name, table_name, row);
                position += wr.len() as u32;
                vec![tm, wr]
            }
            WalRecord::UpdateRows {
                table,
                assignments,
                where_column,
                where_value,
            } if assignments.len() <= 255 => {
                let (schema_name, table_name) = split_schema_table(schema, table);
                let tm = encode_table_map_event(
                    server_id,
                    position,
                    schema_name,
                    table_name,
                    assignments.len().max(1) as u8,
                );
                position += tm.len() as u32;
                let ur = encode_update_rows_event(
                    &RowEventEncode {
                        server_id,
                        position,
                        schema: schema_name,
                        table: table_name,
                        image,
                        extra: &[],
                    },
                    assignments,
                    where_column.as_deref(),
                    where_value.as_deref(),
                );
                position += ur.len() as u32;
                vec![tm, ur]
            }
            WalRecord::DeleteRows {
                table,
                column,
                value,
            } => {
                let (schema_name, table_name) = split_schema_table(schema, table);
                let tm = encode_table_map_event(server_id, position, schema_name, table_name, 1);
                position += tm.len() as u32;
                let dr = encode_delete_rows_event(
                    &RowEventEncode {
                        server_id,
                        position,
                        schema: schema_name,
                        table: table_name,
                        image,
                        extra: &[],
                    },
                    column.as_deref(),
                    value.as_deref(),
                );
                position += dr.len() as u32;
                vec![tm, dr]
            }
            _ => {
                if let Some(sql) = wal_record_to_sql(record) {
                    let qe = encode_query_event(server_id, position, schema, &sql);
                    position += qe.len() as u32;
                    vec![qe]
                } else {
                    Vec::new()
                }
            }
        };
        events.extend(event);
    }
    Ok(events)
}

const GTID_LOG_EVENT_BODY_FIXED: usize = 1 + 16 + 8 + 1 + 8 + 8 + 7 + 4;

fn gtid_log_event_wire_length(dml_bytes: u64) -> usize {
    let mut gtid_len = EVENT_HEADER_LEN + GTID_LOG_EVENT_BODY_FIXED + 1;
    for _ in 0..5 {
        let tx_length = gtid_len as u64 + dml_bytes;
        let packed = packed_transaction_length_size(tx_length);
        gtid_len = EVENT_HEADER_LEN + GTID_LOG_EVENT_BODY_FIXED + packed;
    }
    gtid_len
}

fn gtid_log_event_body_length(tx_length: u64) -> usize {
    GTID_LOG_EVENT_BODY_FIXED + packed_transaction_length_size(tx_length)
}

fn packed_transaction_length_size(n: u64) -> usize {
    if n < 251 {
        1
    } else if n < 65536 {
        3
    } else if n < 16_777_216 {
        4
    } else if n < 4_294_967_296 {
        5
    } else {
        9
    }
}

fn append_packed_transaction_length(body: &mut Vec<u8>, n: u64) {
    if n < 251 {
        body.push(n as u8);
    } else if n < 65536 {
        body.push(0xfc);
        body.extend_from_slice(&(n as u16).to_le_bytes());
    } else if n < 16_777_216 {
        body.push(0xfd);
        body.extend_from_slice(&(n as u32).to_le_bytes()[..3]);
    } else if n < 4_294_967_296 {
        body.push(0xfe);
        body.extend_from_slice(&(n as u32).to_le_bytes());
    } else {
        body.push(0xff);
        body.extend_from_slice(&n.to_le_bytes());
    }
}

fn parse_uuid_bytes(uuid: &str) -> Option<[u8; 16]> {
    if let Some(prefix) = uuid.strip_suffix("-0000-0000-0000-000000000001") {
        if prefix.len() == 16 && prefix.chars().all(|c| c.is_ascii_hexdigit()) {
            let val = u64::from_str_radix(prefix, 16).ok()?;
            let mut sid = [0u8; 16];
            sid[0..8].copy_from_slice(&val.to_be_bytes());
            sid[15] = 1;
            return Some(sid);
        }
    }
    let hex: String = uuid.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut sid = [0u8; 16];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let byte = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
        sid[i] = byte;
    }
    Some(sid)
}

fn sid_to_uuid_string(sid: &[u8; 16]) -> String {
    if sid[8..15].iter().all(|&b| b == 0) && sid[15] == 1 {
        let val = u64::from_be_bytes(sid[0..8].try_into().expect("sid prefix"));
        format!("{:016x}-0000-0000-0000-000000000001", val)
    } else {
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            sid[0], sid[1], sid[2], sid[3], sid[4], sid[5], sid[6], sid[7], sid[8], sid[9],
            sid[10], sid[11], sid[12], sid[13], sid[14], sid[15],
        )
    }
}

fn format_gtid_from_sid(sid: &[u8; 16], gno: u64) -> String {
    format!("{}:{gno}", sid_to_uuid_string(sid))
}

fn parse_gtid_string(gtid: &str) -> Option<([u8; 16], u64)> {
    let (uuid, gno_str) = gtid.rsplit_once(':')?;
    let sid = parse_uuid_bytes(uuid)?;
    let gno = gno_str.parse().ok()?;
    Some((sid, gno))
}

fn encode_gtid_log_event(
    server_id: u32,
    position: u32,
    gtid: &str,
    tx_length: u64,
) -> Result<Vec<u8>, StorageError> {
    let (sid, gno) = parse_gtid_string(gtid)
        .ok_or_else(|| StorageError::Message(format!("invalid gtid for binlog: {gtid}")))?;
    let mut body = Vec::new();
    body.push(0); // row-only commit (no FLAG_MAY_HAVE_SBR)
    body.extend_from_slice(&sid);
    body.extend_from_slice(&gno.to_le_bytes());
    body.push(GTID_LOGICAL_TIMESTAMP_TYPECODE);
    body.extend_from_slice(&0u64.to_le_bytes()); // last_committed
    body.extend_from_slice(&gno.to_le_bytes()); // sequence_number tracks gno in MVP
    body.extend_from_slice(&[0u8; 7]); // immediate_commit_timestamp (u56 LE)
    append_packed_transaction_length(&mut body, tx_length);
    body.extend_from_slice(&GTID_IMMEDIATE_SERVER_VERSION.to_le_bytes());
    Ok(encode_event(
        EVENT_TYPE_GTID_LOG,
        server_id,
        position,
        0,
        &body,
    ))
}

/// Parse a rusql `GTID_LOG_EVENT` body (post-common-header payload) into `uuid:seq` text.
pub fn parse_gtid_log_event(body: &[u8]) -> Option<String> {
    if body.len() < 1 + 16 + 8 {
        return None;
    }
    let sid: [u8; 16] = body[1..17].try_into().ok()?;
    let gno = u64::from_le_bytes(body[17..25].try_into().ok()?);
    Some(format_gtid_from_sid(&sid, gno))
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

fn update_sql(
    table: &str,
    assignments: &[(String, String)],
    where_clause: Option<&(String, String)>,
) -> String {
    let sets: Vec<String> = assignments
        .iter()
        .map(|(col, val)| format!("{} = {}", col, sql_literal(val)))
        .collect();
    let mut sql = format!("UPDATE {table} SET {}", sets.join(", "));
    if let Some((col, val)) = where_clause {
        sql.push_str(&format!(" WHERE {col} = {}", sql_literal(val)));
    }
    sql
}

fn delete_sql(table: &str, where_clause: Option<&(String, String)>) -> String {
    let mut sql = format!("DELETE FROM {table}");
    if let Some((col, val)) = where_clause {
        sql.push_str(&format!(" WHERE {col} = {}", sql_literal(val)));
    }
    sql
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

fn append_rows_v2_prefix(body: &mut Vec<u8>, schema: &str, table: &str, extra: &[u8]) {
    body.extend_from_slice(&table_id_for(schema, table).to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    let extra = &extra[..extra.len().min(65533)];
    let extra_len = u16::try_from(2 + extra.len()).unwrap_or(2);
    body.extend_from_slice(&extra_len.to_le_bytes());
    body.extend_from_slice(extra);
}

/// Split a rusql v2 row-event body into `(table_id, payload after extra data)`.
///
/// Layout: `table_id` u64, `flags` u16, `extra_len` u16 (includes itself, minimum 2), extra bytes.
fn split_rows_v2_body(body: &[u8]) -> Result<(u64, &[u8]), ()> {
    if body.len() < 12 {
        return Err(());
    }
    let table_id = u64::from_le_bytes(body[0..8].try_into().map_err(|_| ())?);
    let extra_len = u16::from_le_bytes(body[10..12].try_into().map_err(|_| ())?) as usize;
    if extra_len < 2 {
        return Err(());
    }
    let payload_at = 10 + extra_len;
    if payload_at > body.len() {
        return Err(());
    }
    Ok((table_id, &body[payload_at..]))
}

fn encode_write_rows_event(
    server_id: u32,
    position: u32,
    schema: &str,
    table: &str,
    row: &[String],
) -> Vec<u8> {
    let mut body = Vec::new();
    append_rows_v2_prefix(&mut body, schema, table, &[]);
    append_write_row_cells(&mut body, row);
    encode_event(EVENT_TYPE_WRITE_ROWS_V2, server_id, position, 0, &body)
}

/// Full-row INSERT image. `binlog_row_image` does not drop INSERT columns.
fn append_write_row_cells(body: &mut Vec<u8>, row: &[String]) {
    let width = row.len();
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
}

fn append_lenstr(body: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    body.extend_from_slice(bytes);
}

fn read_lenstr(body: &[u8], offset: &mut usize) -> Option<String> {
    if *offset + 4 > body.len() {
        return None;
    }
    let len = u32::from_le_bytes(body[*offset..*offset + 4].try_into().ok()?) as usize;
    *offset += 4;
    if *offset + len > body.len() {
        return None;
    }
    let s = String::from_utf8_lossy(&body[*offset..*offset + len]).into_owned();
    *offset += len;
    Some(s)
}

fn wal_update_rows(
    assignments: &[ColumnAssignment],
    where_column: Option<&str>,
    where_value: Option<&str>,
) -> (Vec<ImageColumn>, Vec<ImageColumn>) {
    let mut before = Vec::new();
    let mut after = Vec::new();
    if let (Some(col), Some(val)) = (where_column, where_value) {
        let key = ImageColumn {
            name: col.to_string(),
            value: val.to_string(),
            is_blob: false,
            primary_key: true,
        };
        before.push(key.clone());
        after.push(key);
    }
    for assignment in assignments {
        if let Some(existing) = after.iter_mut().find(|c| c.name == assignment.column) {
            existing.value.clone_from(&assignment.value);
        } else {
            after.push(ImageColumn {
                name: assignment.column.clone(),
                value: assignment.value.clone(),
                is_blob: false,
                primary_key: false,
            });
        }
    }
    (before, after)
}

struct RowEventEncode<'a> {
    server_id: u32,
    position: u32,
    schema: &'a str,
    table: &'a str,
    image: BinlogRowImage,
    extra: &'a [u8],
}

fn encode_update_rows_event(
    event: &RowEventEncode<'_>,
    assignments: &[ColumnAssignment],
    where_column: Option<&str>,
    where_value: Option<&str>,
) -> Vec<u8> {
    let (before_row, after_row) = wal_update_rows(assignments, where_column, where_value);
    encode_partial_update_event(event, &before_row, &after_row)
}

/// Encode one v2 `UPDATE_ROWS` event. `before_row` / `after_row` are full known images;
/// `event.image` selects which columns are written. `event.extra` is MySQL v2 extra data
/// (not including the length prefix).
fn encode_partial_update_event(
    event: &RowEventEncode<'_>,
    before_row: &[ImageColumn],
    after_row: &[ImageColumn],
) -> Vec<u8> {
    let (before, after) = partial_update_images(before_row, after_row, event.image);
    let mut body = Vec::new();
    append_rows_v2_prefix(&mut body, event.schema, event.table, event.extra);
    body.push(event.image.tag());
    append_image_columns(&mut body, &before);
    append_image_columns(&mut body, &after);
    encode_event(
        EVENT_TYPE_UPDATE_ROWS_V2,
        event.server_id,
        event.position,
        0,
        &body,
    )
}

fn encode_delete_rows_event(
    event: &RowEventEncode<'_>,
    column: Option<&str>,
    value: Option<&str>,
) -> Vec<u8> {
    let before_row = match (column, value) {
        (Some(col), Some(val)) => vec![ImageColumn {
            name: col.to_string(),
            value: val.to_string(),
            is_blob: false,
            primary_key: true,
        }],
        _ => Vec::new(),
    };
    encode_partial_delete_event(event, &before_row)
}

/// Encode one v2 `DELETE_ROWS` event from a full before-image.
fn encode_partial_delete_event(event: &RowEventEncode<'_>, before_row: &[ImageColumn]) -> Vec<u8> {
    let (before, _) = partial_update_images(before_row, before_row, event.image);
    let mut body = Vec::new();
    append_rows_v2_prefix(&mut body, event.schema, event.table, event.extra);
    body.push(event.image.tag());
    append_image_columns(&mut body, &before);
    encode_event(
        EVENT_TYPE_DELETE_ROWS_V2,
        event.server_id,
        event.position,
        0,
        &body,
    )
}

fn append_image_columns(body: &mut Vec<u8>, cols: &[ImageColumn]) {
    let n = u8::try_from(cols.len()).unwrap_or(u8::MAX);
    let cols = &cols[..usize::from(n).min(cols.len())];
    body.push(n);
    for col in cols {
        append_lenstr(body, &col.name);
        append_lenstr(body, &col.value);
        let mut flags = 0u8;
        if col.is_blob {
            flags |= IMAGE_FLAG_BLOB;
        }
        if col.primary_key {
            flags |= IMAGE_FLAG_PK;
        }
        body.push(flags);
    }
}

fn read_image_columns(body: &[u8], offset: &mut usize) -> Option<Vec<ImageColumn>> {
    let n = *body.get(*offset)? as usize;
    *offset += 1;
    let mut cols = Vec::with_capacity(n);
    for _ in 0..n {
        let name = read_lenstr(body, offset)?;
        let value = read_lenstr(body, offset)?;
        let flags = *body.get(*offset)?;
        *offset += 1;
        cols.push(ImageColumn {
            name,
            value,
            is_blob: flags & IMAGE_FLAG_BLOB != 0,
            primary_key: flags & IMAGE_FLAG_PK != 0,
        });
    }
    Some(cols)
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
    let row = parse_write_row_payload(&body[10..])?;
    Some((table_id, row))
}

fn parse_write_row_payload(payload: &[u8]) -> Option<Vec<String>> {
    let width = *payload.first()? as usize;
    let mut offset = 1;
    let incl_len = bitmap_len(width);
    if offset + incl_len > payload.len() {
        return None;
    }
    let included = &payload[offset..offset + incl_len];
    offset += incl_len;
    let null_len = bitmap_len(width);
    if offset + null_len > payload.len() {
        return None;
    }
    let nulls = &payload[offset..offset + null_len];
    offset += null_len;
    let mut row = Vec::with_capacity(width);
    for i in 0..width {
        if !bit_is_set(included, i) || bit_is_set(nulls, i) {
            row.push(String::new());
            continue;
        }
        if offset + 4 > payload.len() {
            return None;
        }
        let len = u32::from_le_bytes(payload[offset..offset + 4].try_into().ok()?) as usize;
        offset += 4;
        if offset + len > payload.len() {
            return None;
        }
        row.push(String::from_utf8_lossy(&payload[offset..offset + len]).into_owned());
        offset += len;
    }
    Some(row)
}

struct UpdateRowImage {
    table_id: u64,
    assignments: Vec<(String, String)>,
    where_clause: Option<(String, String)>,
}

fn parse_update_rows(body: &[u8]) -> Option<UpdateRowImage> {
    if body.len() < 11 {
        return None;
    }
    let table_id = u64::from_le_bytes(body[0..8].try_into().ok()?);
    let nset = *body.get(10)? as usize;
    let mut offset = 11;
    let mut assignments = Vec::with_capacity(nset);
    for _ in 0..nset {
        let col = read_lenstr(body, &mut offset)?;
        let val = read_lenstr(body, &mut offset)?;
        assignments.push((col, val));
    }
    let has_where = *body.get(offset)?;
    offset += 1;
    let where_clause = if has_where == 1 {
        let col = read_lenstr(body, &mut offset)?;
        let val = read_lenstr(body, &mut offset)?;
        Some((col, val))
    } else {
        None
    };
    Some(UpdateRowImage {
        table_id,
        assignments,
        where_clause,
    })
}

fn parse_delete_rows(body: &[u8]) -> Option<(u64, Option<(String, String)>)> {
    if body.len() < 11 {
        return None;
    }
    let table_id = u64::from_le_bytes(body[0..8].try_into().ok()?);
    let has_where = *body.get(10)?;
    let mut offset = 11;
    let where_clause = if has_where == 1 {
        let col = read_lenstr(body, &mut offset)?;
        let val = read_lenstr(body, &mut offset)?;
        Some((col, val))
    } else {
        None
    };
    Some((table_id, where_clause))
}

enum ReconstructedDml {
    Insert {
        table_id: u64,
        row: Vec<String>,
    },
    Update {
        table_id: u64,
        before: Vec<ImageColumn>,
        after: Vec<ImageColumn>,
    },
    Delete {
        table_id: u64,
        before: Vec<ImageColumn>,
    },
}

impl ReconstructedDml {
    fn table_id(&self) -> u64 {
        match self {
            Self::Insert { table_id, .. }
            | Self::Update { table_id, .. }
            | Self::Delete { table_id, .. } => *table_id,
        }
    }
}

fn corrupt_row_event() -> RowEventError {
    RowEventError {
        code: ER_SLAVE_CORRUPT_EVENT,
        message: rusql_i18n::messages::sql_slave_corrupt_event(),
    }
}

/// Decode one rusql v2 `UPDATE_ROWS` event, including a non-empty extra-data header.
pub fn decode_update_rows_v2(event: &[u8]) -> Result<DecodedUpdateRows, RowEventError> {
    if event.len() < EVENT_HEADER_LEN || event[4] != EVENT_TYPE_UPDATE_ROWS_V2 {
        return Err(corrupt_row_event());
    }
    decode_update_v2_body(&event[EVENT_HEADER_LEN..]).map_err(|()| corrupt_row_event())
}

fn decode_update_v2_body(body: &[u8]) -> Result<DecodedUpdateRows, ()> {
    let (table_id, payload) = split_rows_v2_body(body)?;
    if payload.is_empty() {
        return Err(());
    }
    let image = BinlogRowImage::from_tag(payload[0]).ok_or(())?;
    let mut offset = 1;
    let before = read_image_columns(payload, &mut offset).ok_or(())?;
    let after = read_image_columns(payload, &mut offset).ok_or(())?;
    Ok(DecodedUpdateRows {
        table_id,
        image,
        before,
        after,
    })
}

fn parse_delete_v2_payload(payload: &[u8]) -> Option<Vec<ImageColumn>> {
    let _image = BinlogRowImage::from_tag(*payload.first()?)?;
    let mut offset = 1;
    read_image_columns(payload, &mut offset)
}

fn image_col(name: String, value: String, primary_key: bool) -> ImageColumn {
    ImageColumn {
        name,
        value,
        is_blob: false,
        primary_key,
    }
}

fn dml_from_row_event(event_type: u8, body: &[u8]) -> Result<Option<ReconstructedDml>, ()> {
    match event_type {
        EVENT_TYPE_WRITE_ROWS_V1 => Ok(parse_write_rows(body)
            .map(|(table_id, row)| ReconstructedDml::Insert { table_id, row })),
        EVENT_TYPE_WRITE_ROWS_V2 => {
            let (table_id, payload) = split_rows_v2_body(body)?;
            let row = parse_write_row_payload(payload).ok_or(())?;
            Ok(Some(ReconstructedDml::Insert { table_id, row }))
        }
        EVENT_TYPE_UPDATE_ROWS_V1 => Ok(parse_update_rows(body).map(|image| {
            let before = image
                .where_clause
                .into_iter()
                .map(|(name, value)| image_col(name, value, true))
                .collect();
            let after = image
                .assignments
                .into_iter()
                .map(|(name, value)| image_col(name, value, false))
                .collect();
            ReconstructedDml::Update {
                table_id: image.table_id,
                before,
                after,
            }
        })),
        EVENT_TYPE_UPDATE_ROWS_V2 => {
            let decoded = decode_update_v2_body(body)?;
            Ok(Some(ReconstructedDml::Update {
                table_id: decoded.table_id,
                before: decoded.before,
                after: decoded.after,
            }))
        }
        EVENT_TYPE_DELETE_ROWS_V1 => Ok(parse_delete_rows(body).map(|(table_id, where_clause)| {
            let before = where_clause
                .into_iter()
                .map(|(name, value)| image_col(name, value, true))
                .collect();
            ReconstructedDml::Delete { table_id, before }
        })),
        EVENT_TYPE_DELETE_ROWS_V2 => {
            let (table_id, payload) = split_rows_v2_body(body)?;
            let before = parse_delete_v2_payload(payload).ok_or(())?;
            Ok(Some(ReconstructedDml::Delete { table_id, before }))
        }
        _ => Ok(None),
    }
}

fn sql_for_dml(schema: &str, table: &str, dml: &ReconstructedDml) -> String {
    let storage_key = table_storage_key(schema, table);
    match dml {
        ReconstructedDml::Insert { row, .. } => insert_sql(&storage_key, row),
        ReconstructedDml::Update { before, after, .. } => {
            update_sql_from_images(&storage_key, before, after)
        }
        ReconstructedDml::Delete { before, .. } => delete_sql_from_images(&storage_key, before),
    }
}

fn update_sql_from_images(table: &str, before: &[ImageColumn], after: &[ImageColumn]) -> String {
    let sets: Vec<String> = after
        .iter()
        .map(|c| format!("{} = {}", c.name, sql_literal(&c.value)))
        .collect();
    let mut sql = if sets.is_empty() {
        if let Some(col) = before.first() {
            format!(
                "UPDATE {table} SET {} = {}",
                col.name,
                sql_literal(&col.value)
            )
        } else {
            format!("UPDATE {table}")
        }
    } else {
        format!("UPDATE {table} SET {}", sets.join(", "))
    };
    append_image_where(&mut sql, before);
    sql
}

fn delete_sql_from_images(table: &str, before: &[ImageColumn]) -> String {
    let mut sql = format!("DELETE FROM {table}");
    append_image_where(&mut sql, before);
    sql
}

fn append_image_where(sql: &mut String, cols: &[ImageColumn]) {
    if cols.is_empty() {
        return;
    }
    let preds: Vec<String> = cols
        .iter()
        .map(|c| format!("{} = {}", c.name, sql_literal(&c.value)))
        .collect();
    sql.push_str(" WHERE ");
    sql.push_str(&preds.join(" AND "));
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

/// Encode a synthetic `HEARTBEAT_LOG_EVENT` for replication dump (not written to disk).
pub fn encode_heartbeat_log_event(server_id: u32, log_pos: u32) -> Vec<u8> {
    encode_event(EVENT_TYPE_HEARTBEAT, server_id, log_pos, 0, &[])
}

/// Resolve heartbeat interval from optional replica `COM_BINLOG_DUMP` period (nanoseconds).
pub fn master_heartbeat_period_from_dump(heartbeat_period_ns: Option<u64>) -> std::time::Duration {
    if let Some(ns) = heartbeat_period_ns.filter(|&n| n > 0) {
        let secs = (ns / 1_000_000_000).clamp(1, 3600);
        return std::time::Duration::from_secs(secs);
    }
    master_heartbeat_period_default()
}

/// Server default when the dump command carries no heartbeat period.
pub fn master_heartbeat_period_default() -> std::time::Duration {
    #[cfg(test)]
    {
        std::time::Duration::from_millis(500)
    }
    #[cfg(not(test))]
    {
        std::env::var("RUSQL_MASTER_HEARTBEAT_PERIOD_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|&s| s > 0)
            .map(std::time::Duration::from_secs)
            .unwrap_or(std::time::Duration::from_secs(
                DEFAULT_MASTER_HEARTBEAT_PERIOD_SECS,
            ))
    }
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

/// Extract QUERY_EVENT SQL and DML reconstructed from TABLE_MAP + row events.
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
            EVENT_TYPE_WRITE_ROWS_V1
            | EVENT_TYPE_WRITE_ROWS_V2
            | EVENT_TYPE_UPDATE_ROWS_V1
            | EVENT_TYPE_UPDATE_ROWS_V2
            | EVENT_TYPE_DELETE_ROWS_V1
            | EVENT_TYPE_DELETE_ROWS_V2 => {
                if let Ok(Some(dml)) = dml_from_row_event(event[4], body) {
                    if let Some((schema, table)) = table_maps.get(&dml.table_id()) {
                        queries.push(sql_for_dml(schema, table, &dml));
                    }
                }
            }
            EVENT_TYPE_GTID_LOG | EVENT_TYPE_HEARTBEAT => {}
            _ => {}
        }
    }
    queries
}

/// Replay binlog bytes, optionally skipping transactions whose GTID is already in `gtid`.
pub fn apply_binlog_bytes<F>(
    data: &[u8],
    mut gtid: Option<&mut GtidState>,
    mut apply_sql: F,
) -> Result<u64, StorageError>
where
    F: FnMut(&str, &str) -> Result<(), StorageError>,
{
    let mut table_maps: HashMap<u64, (String, String)> = HashMap::new();
    let mut applied = 0u64;
    let mut skip_trx = false;
    let mut current_gtid: Option<String> = None;
    let mut dml_in_trx = false;

    for event in events_from_position(data, 4) {
        if event.len() < EVENT_HEADER_LEN {
            continue;
        }
        let body = &event[EVENT_HEADER_LEN..];
        match event[4] {
            EVENT_TYPE_GTID_LOG => {
                if dml_in_trx {
                    if let (Some(g), Some(state)) = (&current_gtid, gtid.as_mut()) {
                        state.mark_applied(g);
                    }
                    dml_in_trx = false;
                }
                current_gtid = parse_gtid_log_event(body);
                skip_trx = current_gtid
                    .as_ref()
                    .map(|g| gtid.as_ref().map(|s| s.is_applied(g)).unwrap_or(false))
                    .unwrap_or(false);
            }
            EVENT_TYPE_QUERY if !skip_trx => {
                if body.len() > 13 {
                    let schema_len = body[12] as usize;
                    let query_start = 13 + schema_len + 1;
                    if query_start <= body.len() {
                        let sql = String::from_utf8_lossy(&body[query_start..]).to_string();
                        let (sql, _) = strip_gtid_comment(&sql);
                        if !sql.is_empty() && !sql.starts_with("/*") {
                            apply_sql("rusql", sql)?;
                            applied += 1;
                            dml_in_trx = true;
                        }
                    }
                }
            }
            EVENT_TYPE_TABLE_MAP => {
                if let Some((table_id, schema, table)) = parse_table_map(body) {
                    table_maps.insert(table_id, (schema, table));
                }
            }
            EVENT_TYPE_WRITE_ROWS_V1
            | EVENT_TYPE_WRITE_ROWS_V2
            | EVENT_TYPE_UPDATE_ROWS_V1
            | EVENT_TYPE_UPDATE_ROWS_V2
            | EVENT_TYPE_DELETE_ROWS_V1
            | EVENT_TYPE_DELETE_ROWS_V2
                if !skip_trx =>
            {
                match dml_from_row_event(event[4], body) {
                    Ok(Some(dml)) => {
                        if let Some((schema, table)) = table_maps.get(&dml.table_id()) {
                            apply_sql("rusql", &sql_for_dml(schema, table, &dml))?;
                            applied += 1;
                            dml_in_trx = true;
                        }
                    }
                    Ok(None) => {}
                    Err(()) => {
                        return Err(StorageError::Message(
                            rusql_i18n::messages::sql_slave_corrupt_event(),
                        ));
                    }
                }
            }
            EVENT_TYPE_HEARTBEAT => {}
            _ => {}
        }
    }
    if dml_in_trx {
        if let (Some(g), Some(state)) = (current_gtid, gtid.as_mut()) {
            state.mark_applied(&g);
        }
    }
    Ok(applied)
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
    dump_events_with_next_position(data, position).0
}

/// Dump packets from `position` plus the file offset after the last complete dumped event.
///
/// If no complete event is available, the next position is unchanged so a follower can retry.
pub fn dump_events_with_next_position(data: &[u8], position: u32) -> (Vec<Vec<u8>>, u32) {
    let mut packets = Vec::new();
    if data.len() < EVENT_HEADER_LEN {
        return (packets, position);
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
    let mut next = position;
    while offset + EVENT_HEADER_LEN <= data.len() {
        let event_len =
            u32::from_le_bytes(data[offset + 9..offset + 13].try_into().unwrap()) as usize;
        if event_len < EVENT_HEADER_LEN || offset + event_len > data.len() {
            break;
        }
        if offset >= start {
            let event = &data[offset..offset + event_len];
            let mut packet = Vec::with_capacity(1 + event.len());
            packet.push(0x00);
            packet.extend_from_slice(event);
            packets.push(packet);
            next = (offset + event_len) as u32;
        }
        offset += event_len;
    }
    (packets, next)
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
        let (again, next) = dump_events_with_next_position(&bytes, 4);
        assert_eq!(again, packets);
        assert_eq!(
            next as usize,
            4 + from_start.iter().map(|e| e.len()).sum::<usize>()
        );
        let after_fde = 4 + from_start[0].len();
        let rest = events_from_position(&bytes, after_fde as u32);
        assert!(rest.iter().any(|e| e[4] == EVENT_TYPE_QUERY));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn master_binlog_status_tracks_writer_position_after_commit() {
        let dir = std::env::temp_dir().join(format!("rusql-master-status-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut writer = BinlogWriter::open(&dir, 1).unwrap();
        let before = master_binlog_status(&dir).expect("binlog file");
        assert_eq!(before.file, "binlog.000001");
        assert_eq!(before.position, writer.current_position());
        let record = WalRecord::from_insert("t", vec!["1".into()]);
        writer.append_commit(&dir, "rusql", &[record]).unwrap();
        let after = master_binlog_status(&dir).expect("binlog file");
        assert_eq!(after.file, "binlog.000001");
        assert_eq!(after.position, writer.current_position());
        assert!(after.position > before.position);
        let _ = std::fs::remove_dir_all(&dir);
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
        let gtid_idx = events
            .iter()
            .position(|e| e[4] == EVENT_TYPE_GTID_LOG)
            .expect("GTID_LOG_EVENT");
        let tm_idx = events
            .iter()
            .position(|e| e[4] == EVENT_TYPE_TABLE_MAP)
            .expect("TABLE_MAP");
        assert!(
            gtid_idx < tm_idx,
            "GTID must precede row events, got order {gtid_idx}/{tm_idx}"
        );
        assert!(events.iter().any(|e| e[4] == EVENT_TYPE_WRITE_ROWS_V2));
        assert!(events.iter().all(|e| e[4] != EVENT_TYPE_QUERY));
        let packets = dump_event_packets(&bytes, 4);
        assert!(packets
            .iter()
            .any(|p| p.get(5) == Some(&EVENT_TYPE_TABLE_MAP)));
        assert!(packets
            .iter()
            .any(|p| p.get(5) == Some(&EVENT_TYPE_WRITE_ROWS_V2)));
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
    fn binlog_writer_update_emits_update_rows() {
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
                use_column_default: false,
            }],
            Some(&crate::DeleteFilter {
                column: "id".into(),
                value: "1".into(),
            }),
        );
        writer.append_commit(&dir, "rusql", &[record]).unwrap();
        let bytes = read_binlog_file(writer.current_path()).unwrap();
        let events = events_from_position(&bytes, 4);
        assert!(events.iter().any(|e| e[4] == EVENT_TYPE_TABLE_MAP));
        assert!(events.iter().any(|e| e[4] == EVENT_TYPE_UPDATE_ROWS_V2));
        assert!(events.iter().all(|e| e[4] != EVENT_TYPE_QUERY));
        let queries = extract_query_events(&bytes);
        assert_eq!(queries.len(), 1);
        assert!(queries[0].contains("UPDATE t SET"));
        assert!(queries[0].contains("WHERE id = 1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binlog_writer_delete_emits_delete_rows() {
        let dir =
            std::env::temp_dir().join(format!("rusql-binlog-writer-delete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut writer = BinlogWriter::open(&dir, 1).unwrap();
        let record = WalRecord::from_delete(
            "t",
            Some(&crate::DeleteFilter {
                column: "id".into(),
                value: "1".into(),
            }),
        );
        writer.append_commit(&dir, "rusql", &[record]).unwrap();
        let bytes = read_binlog_file(writer.current_path()).unwrap();
        let events = events_from_position(&bytes, 4);
        assert!(events.iter().any(|e| e[4] == EVENT_TYPE_TABLE_MAP));
        assert!(events.iter().any(|e| e[4] == EVENT_TYPE_DELETE_ROWS_V2));
        assert!(events.iter().all(|e| e[4] != EVENT_TYPE_QUERY));
        let queries = extract_query_events(&bytes);
        assert_eq!(queries.len(), 1);
        assert!(queries[0].contains("DELETE FROM t"));
        assert!(queries[0].contains("WHERE id = 1"));
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
                use_column_default: false,
            }],
            Some(&crate::DeleteFilter {
                column: "id".into(),
                value: "1".into(),
            }),
        ))
        .unwrap();
        assert!(upd.contains("UPDATE t SET"));
    }

    #[test]
    fn list_binary_logs_empty_when_dir_missing() {
        let dir = std::env::temp_dir().join(format!(
            "rusql-list-binary-logs-missing-{}-{}",
            std::process::id(),
            1u32
        ));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(list_binary_logs(&dir).is_empty());
    }

    #[test]
    fn list_binary_logs_current_file_after_open() {
        let dir = std::env::temp_dir().join(format!(
            "rusql-list-binary-logs-open-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let writer = BinlogWriter::open(&dir, 1).unwrap();
        let logs = list_binary_logs(&dir);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].name, "binlog.000001");
        assert!(logs[0].size > 0);
        drop(writer);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_binlog_events_format_desc_from_open_file() {
        let dir = std::env::temp_dir().join(format!(
            "rusql-list-binlog-events-open-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let writer = BinlogWriter::open(&dir, 7).unwrap();
        let rows = list_binlog_events(&dir, None, None);
        assert!(!rows.is_empty());
        assert_eq!(rows[0].log_name, "binlog.000001");
        assert_eq!(rows[0].pos, 4);
        assert_eq!(rows[0].event_type, "Format_desc");
        assert_eq!(rows[0].server_id, 7);
        assert!(rows[0].end_log_pos > rows[0].pos);
        assert!(rows[0].info.contains("Binlog ver: 4"));
        drop(writer);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_binlog_events_unknown_type_does_not_crash() {
        let dir = std::env::temp_dir().join(format!(
            "rusql-list-binlog-events-unknown-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("binlog")).unwrap();
        let mut data = BINLOG_MAGIC.to_vec();
        let event_len: u32 = EVENT_HEADER_LEN as u32;
        data.extend_from_slice(&0u32.to_le_bytes());
        data.push(99);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&event_len.to_le_bytes());
        data.extend_from_slice(&(4u32 + event_len).to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        std::fs::write(dir.join("binlog").join("binlog.000001"), &data).unwrap();
        let rows = list_binlog_events(&dir, Some("binlog.000001"), None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].event_type, "Unknown");
        assert_eq!(rows[0].info, "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gtid_event_pins_header_layout() {
        let gtid = "a1b2c3d4-e5f6-4789-a012-3456789abcde:42";
        let event = encode_gtid_log_event(7, 128, gtid, 200).unwrap();
        assert_eq!(
            event.len(),
            EVENT_HEADER_LEN + gtid_log_event_body_length(200)
        );
        assert_eq!(event[4], EVENT_TYPE_GTID_LOG);
        assert_eq!(&event[5..9], &7u32.to_le_bytes());
        assert_eq!(
            parse_gtid_log_event(&event[EVENT_HEADER_LEN..]).as_deref(),
            Some(gtid)
        );
        // Payload prefix: flags, UUID (network order hex), gno LE.
        let body = &event[EVENT_HEADER_LEN..];
        assert_eq!(body[0], 0);
        assert_eq!(
            &body[1..17],
            &[
                0xa1, 0xb2, 0xc3, 0xd4, 0xe5, 0xf6, 0x47, 0x89, 0xa0, 0x12, 0x34, 0x56, 0x78, 0x9a,
                0xbc, 0xde
            ]
        );
        assert_eq!(u64::from_le_bytes(body[17..25].try_into().unwrap()), 42);
        assert_eq!(body[25], GTID_LOGICAL_TIMESTAMP_TYPECODE);
    }

    #[test]
    fn gtid_event_skip_already_applied() {
        let dir = std::env::temp_dir().join(format!("rusql-gtid-skip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut writer = BinlogWriter::open(&dir, 1).unwrap();
        writer.gtid_state_mut().server_uuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".into();
        writer.gtid_state_mut().sequence = 0;
        let record = WalRecord::from_insert("t", vec!["1".into()]);
        writer.append_commit(&dir, "rusql", &[record]).unwrap();
        let bytes = read_binlog_file(writer.current_path()).unwrap();
        let gtid_body = events_from_position(&bytes, 4)
            .into_iter()
            .find(|e| e[4] == EVENT_TYPE_GTID_LOG)
            .unwrap()[EVENT_HEADER_LEN..]
            .to_vec();
        let gtid = parse_gtid_log_event(&gtid_body).unwrap();
        let mut state = GtidState {
            server_uuid: writer.gtid_state().server_uuid.clone(),
            sequence: writer.gtid_state().sequence,
            applied: vec![gtid.clone()],
        };
        let mut calls = 0;
        apply_binlog_bytes(&bytes, Some(&mut state), |_schema, _sql| {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, 0, "already-applied GTID should skip DML");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_binlog_events_empty_when_dir_missing() {
        let dir = std::env::temp_dir().join(format!(
            "rusql-list-binlog-events-missing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(list_binlog_events(&dir, None, None).is_empty());
    }

    #[test]
    fn heartbeat_event_pins_header_layout() {
        let event = encode_heartbeat_log_event(7, 128);
        assert_eq!(event.len(), EVENT_HEADER_LEN);
        assert_eq!(event[4], EVENT_TYPE_HEARTBEAT);
        assert_eq!(
            u32::from_le_bytes(event[5..9].try_into().unwrap()),
            7,
            "server_id"
        );
        assert_eq!(
            u32::from_le_bytes(event[9..13].try_into().unwrap()),
            EVENT_HEADER_LEN as u32,
            "event_length"
        );
        assert_eq!(
            u32::from_le_bytes(event[13..17].try_into().unwrap()),
            128 + EVENT_HEADER_LEN as u32,
            "next_log_pos"
        );
    }

    #[test]
    fn heartbeat_does_not_apply_as_dml() {
        let event = encode_heartbeat_log_event(1, 4);
        let mut magic = BINLOG_MAGIC.to_vec();
        magic.extend_from_slice(&event);
        let mut calls = 0u64;
        let applied = apply_binlog_bytes(&magic, None, |_schema, _sql| {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(applied, 0);
        assert_eq!(calls, 0);
        assert!(extract_query_events(&magic).is_empty());
    }

    #[test]
    fn master_heartbeat_period_from_dump_honors_nanoseconds() {
        let period = master_heartbeat_period_from_dump(Some(3_000_000_000));
        assert_eq!(period, std::time::Duration::from_secs(3));
    }

    fn sample_row() -> (Vec<ImageColumn>, Vec<ImageColumn>) {
        let before = vec![
            ImageColumn {
                name: "id".into(),
                value: "1".into(),
                is_blob: false,
                primary_key: true,
            },
            ImageColumn {
                name: "name".into(),
                value: "alice".into(),
                is_blob: false,
                primary_key: false,
            },
            ImageColumn {
                name: "note".into(),
                value: "keep-me".into(),
                is_blob: true,
                primary_key: false,
            },
        ];
        let mut after = before.clone();
        after[1].value = "bob".into();
        (before, after)
    }

    #[test]
    fn row_event_v2() {
        assert_eq!(
            parse_binlog_row_image("full").unwrap(),
            BinlogRowImage::Full
        );
        assert_eq!(
            parse_binlog_row_image("MINIMAL").unwrap(),
            BinlogRowImage::Minimal
        );
        assert_eq!(
            parse_binlog_row_image(" noblob ").unwrap(),
            BinlogRowImage::Noblob
        );
        let bad = parse_binlog_row_image("PARTIAL").unwrap_err();
        assert_eq!(bad.code, crate::ER_WRONG_VALUE_FOR_VAR);
        assert!(bad.message.contains("binlog_row_image"));
        assert!(bad.message.contains("PARTIAL"));

        let set_minimal = rusql_sql::parse("SET @@SESSION.binlog_row_image = 'MINIMAL'")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(
            binlog_row_image_assignment(&set_minimal).unwrap(),
            Some(BinlogRowImage::Minimal)
        );
        let set_bad = rusql_sql::parse("SET GLOBAL binlog_row_image = 'NOPE'")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(
            binlog_row_image_assignment(&set_bad).unwrap_err().code,
            crate::ER_WRONG_VALUE_FOR_VAR
        );
        let other = rusql_sql::parse("SET autocommit = 1")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        assert!(binlog_row_image_assignment(&other).unwrap().is_none());

        let (before, after) = sample_row();
        let extra = [0x00u8, 0x7f, 0x01];
        let event = encode_partial_update_event(
            &RowEventEncode {
                server_id: 1,
                position: 4,
                schema: "rusql",
                table: "t",
                image: BinlogRowImage::Minimal,
                extra: &extra,
            },
            &before,
            &after,
        );
        assert_eq!(event[4], EVENT_TYPE_UPDATE_ROWS_V2);
        let body = &event[EVENT_HEADER_LEN..];
        let extra_len = u16::from_le_bytes(body[10..12].try_into().unwrap());
        assert_eq!(extra_len, 5, "extra length includes the 2-byte prefix");
        assert_eq!(&body[12..15], &extra);
        let decoded = decode_update_rows_v2(&event).expect("v2 extra header must apply");
        assert_eq!(decoded.image, BinlogRowImage::Minimal);
        assert_eq!(
            decoded
                .before
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["id"]
        );
        assert_eq!(
            decoded
                .after
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["name"]
        );

        let mut broken = event.clone();
        broken[EVENT_HEADER_LEN + 10] = 1;
        broken[EVENT_HEADER_LEN + 11] = 0;
        let err = decode_update_rows_v2(&broken).unwrap_err();
        assert_eq!(err.code, ER_SLAVE_CORRUPT_EVENT);

        let mut file = BINLOG_MAGIC.to_vec();
        file.extend_from_slice(&event);
        let applied = apply_binlog_bytes(&file, None, |_schema, sql| {
            assert!(sql.contains("SET name = 'bob'"), "{sql}");
            assert!(sql.contains("WHERE id = 1"), "{sql}");
            assert!(!sql.contains("note"), "{sql}");
            Ok(())
        })
        .unwrap();
        assert_eq!(applied, 0, "no TABLE_MAP, so the row event is not applied");

        let mut mapped = BINLOG_MAGIC.to_vec();
        let tm = encode_table_map_event(1, 4, "rusql", "t", 3);
        let update = encode_partial_update_event(
            &RowEventEncode {
                server_id: 1,
                position: 4 + tm.len() as u32,
                schema: "rusql",
                table: "t",
                image: BinlogRowImage::Minimal,
                extra: &extra,
            },
            &before,
            &after,
        );
        mapped.extend_from_slice(&tm);
        mapped.extend_from_slice(&update);
        let applied = apply_binlog_bytes(&mapped, None, |_schema, sql| {
            assert!(!sql.contains("note"), "{sql}");
            Ok(())
        })
        .unwrap();
        assert_eq!(applied, 1);

        let mut corrupt = BINLOG_MAGIC.to_vec();
        corrupt.extend_from_slice(&tm);
        let mut bad_update = update.clone();
        bad_update[EVENT_HEADER_LEN + 10] = 0;
        bad_update[EVENT_HEADER_LEN + 11] = 0;
        corrupt.extend_from_slice(&bad_update);
        let err = apply_binlog_bytes(&corrupt, None, |_s, _sql| Ok(())).unwrap_err();
        assert!(err.to_string().contains("Corrupted replication event"));

        let dir = std::env::temp_dir().join(format!("rusql-row-v2-insert-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut writer = BinlogWriter::open(&dir, 1).unwrap();
        assert_eq!(writer.row_image(), BinlogRowImage::Full);
        writer.set_row_image(&dir, BinlogRowImage::Minimal).unwrap();
        writer
            .append_commit(
                &dir,
                "rusql",
                &[WalRecord::from_insert(
                    "t",
                    vec!["1".into(), "a".into(), "".into()],
                )],
            )
            .unwrap();
        let bytes = read_binlog_file(writer.current_path()).unwrap();
        assert!(events_from_position(&bytes, 4)
            .iter()
            .any(|e| e[4] == EVENT_TYPE_WRITE_ROWS_V2));
        let queries = extract_query_events(&bytes);
        assert_eq!(
            queries,
            vec!["INSERT INTO t VALUES (1, 'a', NULL)".to_string()]
        );
        let reloaded = BinlogWriter::open(&dir, 1).unwrap();
        assert_eq!(reloaded.row_image(), BinlogRowImage::Minimal);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_rows_partial() {
        let (before, after) = sample_row();
        let (min_before, min_after) =
            partial_update_images(&before, &after, BinlogRowImage::Minimal);
        assert_eq!(min_before.len(), 1);
        assert_eq!(min_before[0].name, "id");
        assert_eq!(min_after.len(), 1);
        assert_eq!(min_after[0].name, "name");
        assert_eq!(min_after[0].value, "bob");

        let (full_before, full_after) =
            partial_update_images(&before, &after, BinlogRowImage::Full);
        assert_eq!(full_before.len(), 3);
        assert_eq!(full_after.len(), 3);

        let (nb_before, nb_after) = partial_update_images(&before, &after, BinlogRowImage::Noblob);
        assert!(nb_before.iter().all(|c| c.name != "note"));
        assert!(nb_after.iter().all(|c| c.name != "note"));
        assert!(nb_after
            .iter()
            .any(|c| c.name == "name" && c.value == "bob"));
        assert!(nb_before.iter().any(|c| c.name == "id"));

        let mut row = std::collections::BTreeMap::from([
            ("id".to_string(), "1".to_string()),
            ("name".to_string(), "alice".to_string()),
            ("note".to_string(), "keep-me".to_string()),
        ]);
        assert!(apply_partial_update(&mut row, &min_before, &min_after));
        assert_eq!(row.get("id").map(String::as_str), Some("1"));
        assert_eq!(row.get("name").map(String::as_str), Some("bob"));
        assert_eq!(row.get("note").map(String::as_str), Some("keep-me"));

        let mut row = std::collections::BTreeMap::from([
            ("id".to_string(), "1".to_string()),
            ("name".to_string(), "alice".to_string()),
            ("note".to_string(), "keep-me".to_string()),
        ]);
        assert!(apply_partial_update(&mut row, &nb_before, &nb_after));
        assert_eq!(row.get("note").map(String::as_str), Some("keep-me"));
        assert_eq!(row.get("name").map(String::as_str), Some("bob"));

        let extra = [0x02u8];
        let event = encode_partial_update_event(
            &RowEventEncode {
                server_id: 1,
                position: 100,
                schema: "app",
                table: "t",
                image: BinlogRowImage::Minimal,
                extra: &extra,
            },
            &before,
            &after,
        );
        let decoded = decode_update_rows_v2(&event).unwrap();
        let sql = update_sql_from_images("app.t", &decoded.before, &decoded.after);
        assert_eq!(sql, "UPDATE app.t SET name = 'bob' WHERE id = 1");
        assert!(!sql.contains("note"));
        assert!(!sql.contains("NULL"));

        let mut mismatch = row.clone();
        mismatch.insert("id".into(), "9".into());
        assert!(!apply_partial_update(
            &mut mismatch,
            &decoded.before,
            &decoded.after
        ));
        assert_eq!(mismatch.get("name").map(String::as_str), Some("bob"));
        assert_eq!(mismatch.get("note").map(String::as_str), Some("keep-me"));
    }
}

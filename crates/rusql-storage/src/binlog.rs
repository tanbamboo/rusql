//! MySQL binlog format spike (M34) — magic header, FORMAT_DESCRIPTION, QUERY_EVENT.
//!
//! This is an experimental subset for replication research, not a production binlog.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::StorageError;

/// Binlog file magic (`0xfe` + `bin`).
pub const BINLOG_MAGIC: [u8; 4] = [0xfe, b'b', b'i', b'n'];

const EVENT_HEADER_LEN: usize = 19;
const EVENT_TYPE_QUERY: u8 = 2;
const EVENT_TYPE_FORMAT_DESCRIPTION: u8 = 15;
const BINLOG_VERSION: u16 = 4;
const SERVER_VERSION: &str = "8.0.33-rusql";

/// Post-header lengths for event types 0..=38 (MySQL 8.0 layout).
const POST_HEADER_LEN: [u8; 40] = [
    0, 13, 0, 8, 4, 18, 0, 0, 0, 0, 0, 0, 0, 0, 0, 56, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Write a minimal binlog file: magic + FORMAT_DESCRIPTION_EVENT + QUERY_EVENT.
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

fn encode_format_description_event(server_id: u32, position: u32) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&BINLOG_VERSION.to_le_bytes());
    let mut version = [0u8; 50];
    let ver = SERVER_VERSION.as_bytes();
    version[..ver.len().min(50)].copy_from_slice(&ver[..ver.len().min(50)]);
    body.extend_from_slice(&version);
    body.extend_from_slice(&0u32.to_le_bytes()); // create_timestamp
    body.push(EVENT_HEADER_LEN as u8);
    body.extend_from_slice(&POST_HEADER_LEN);
    body.push(1); // checksum alg: CRC32 (documented; checksum not appended in spike)

    encode_event(
        EVENT_TYPE_FORMAT_DESCRIPTION,
        server_id,
        position,
        0,
        &body,
    )
}

fn encode_query_event(server_id: u32, position: u32, schema: &str, query: &str) -> Vec<u8> {
    let schema_bytes = schema.as_bytes();
    let query_bytes = query.as_bytes();
    let mut body = Vec::new();
    body.extend_from_slice(&1u32.to_le_bytes()); // thread_id
    body.extend_from_slice(&0u32.to_le_bytes()); // exec_time
    body.push(
        schema_bytes
            .len()
            .try_into()
            .unwrap_or(255),
    );
    body.extend_from_slice(&0u16.to_le_bytes()); // error_code
    body.extend_from_slice(&0u16.to_le_bytes()); // status_vars_len
    body.extend_from_slice(schema_bytes);
    body.push(0); // schema NUL terminator
    body.extend_from_slice(query_bytes);
    encode_event(EVENT_TYPE_QUERY, server_id, position, 0, &body)
}

fn encode_event(
    event_type: u8,
    server_id: u32,
    position: u32,
    flags: u16,
    body: &[u8],
) -> Vec<u8> {
    let event_length = (EVENT_HEADER_LEN + body.len()) as u32;
    let next_log_pos = position + event_length;
    let mut event = Vec::with_capacity(event_length as usize);
    event.extend_from_slice(&0u32.to_le_bytes()); // timestamp
    event.push(event_type);
    event.extend_from_slice(&server_id.to_le_bytes());
    event.extend_from_slice(&event_length.to_le_bytes());
    event.extend_from_slice(&next_log_pos.to_le_bytes());
    event.extend_from_slice(&flags.to_le_bytes());
    event.extend_from_slice(body);
    event
}

/// Return event type byte at `event_offset` (file position after magic).
pub fn event_type_at(data: &[u8], event_offset: usize) -> Option<u8> {
    if event_offset + 4 >= data.len() {
        return None;
    }
    Some(data[event_offset + 4])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn writes_magic_format_description_and_query_event() {
        let path = std::env::temp_dir().join(format!(
            "rusql-binlog-spike-{}-{}.bin",
            std::process::id(),
            1u32
        ));
        let _ = std::fs::remove_file(&path);
        write_binlog_spike(&path, "rusql", "INSERT INTO t VALUES (1)", 1).unwrap();

        let mut bytes = Vec::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(&bytes[..4], BINLOG_MAGIC);
        assert_eq!(event_type_at(&bytes, 4), Some(EVENT_TYPE_FORMAT_DESCRIPTION));
        let fde_len = u32::from_le_bytes(bytes[13..17].try_into().unwrap()) as usize;
        let query_offset = 4 + fde_len;
        assert_eq!(
            event_type_at(&bytes, query_offset),
            Some(EVENT_TYPE_QUERY)
        );
        let payload = String::from_utf8_lossy(&bytes);
        assert!(payload.contains("INSERT INTO t VALUES (1)"));
        assert!(payload.contains("rusql"));
        let _ = std::fs::remove_file(&path);
    }
}

//! Binlog replication dump packet helpers (M174 heartbeat subset).

/// MySQL `HEARTBEAT_LOG_EVENT` type byte.
pub const HEARTBEAT_LOG_EVENT: u8 = 27;

/// `WRITE_ROWS_EVENT` v2 (MySQL type 30).
pub const WRITE_ROWS_EVENT_V2: u8 = 30;
/// `UPDATE_ROWS_EVENT` v2 (MySQL type 31).
pub const UPDATE_ROWS_EVENT_V2: u8 = 31;
/// `DELETE_ROWS_EVENT` v2 (MySQL type 32).
pub const DELETE_ROWS_EVENT_V2: u8 = 32;

/// True for MySQL row-event v2 type codes (30, 31, 32).
pub fn is_rows_v2_event_type(event_type: u8) -> bool {
    matches!(
        event_type,
        WRITE_ROWS_EVENT_V2 | UPDATE_ROWS_EVENT_V2 | DELETE_ROWS_EVENT_V2
    )
}

/// Offset of the row payload in a rusql v2 row-event body.
///
/// Body layout: 8-byte `table_id`, 2-byte flags, then a MySQL v2 `extra_len` u16 that includes
/// itself (minimum 2). Returns `None` when the header is truncated or `extra_len` is less than 2.
pub fn rows_v2_payload_offset(body: &[u8]) -> Option<usize> {
    if body.len() < 12 {
        return None;
    }
    let extra_len = u16::from_le_bytes(body[10..12].try_into().ok()?) as usize;
    if extra_len < 2 {
        return None;
    }
    let at = 10 + extra_len;
    if at > body.len() {
        return None;
    }
    Some(at)
}

/// Standard binlog event header length on the wire.
pub const BINLOG_EVENT_HEADER_LEN: usize = 19;

/// True when `payload` is a dump stream packet (`0x00` + event) carrying a heartbeat.
pub fn is_heartbeat_dump_packet(payload: &[u8]) -> bool {
    payload.first() == Some(&0x00)
        && payload.len() > BINLOG_EVENT_HEADER_LEN
        && payload[1 + 4] == HEARTBEAT_LOG_EVENT
}

/// Validate raw event bytes (without the leading `0x00` dump marker).
pub fn validate_heartbeat_event_bytes(event: &[u8]) -> bool {
    event.len() == BINLOG_EVENT_HEADER_LEN
        && event[4] == HEARTBEAT_LOG_EVENT
        && u32::from_le_bytes(event[9..13].try_into().unwrap_or([0; 4]))
            == BINLOG_EVENT_HEADER_LEN as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_dump_packet_detects_type_27() {
        let mut event = vec![0u8; BINLOG_EVENT_HEADER_LEN];
        event[4] = HEARTBEAT_LOG_EVENT;
        event[9..13].copy_from_slice(&(BINLOG_EVENT_HEADER_LEN as u32).to_le_bytes());
        event[13..17].copy_from_slice(&(BINLOG_EVENT_HEADER_LEN as u32).to_le_bytes());
        let mut packet = vec![0x00];
        packet.extend_from_slice(&event);
        assert!(validate_heartbeat_event_bytes(&event));
        assert!(is_heartbeat_dump_packet(&packet));
    }

    #[test]
    fn row_event_v2_extra_header_is_skipped() {
        assert!(is_rows_v2_event_type(WRITE_ROWS_EVENT_V2));
        assert!(is_rows_v2_event_type(UPDATE_ROWS_EVENT_V2));
        assert!(is_rows_v2_event_type(DELETE_ROWS_EVENT_V2));
        assert!(!is_rows_v2_event_type(23));

        let mut body = vec![0u8; 12];
        body[10..12].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(rows_v2_payload_offset(&body), Some(12));

        let mut with_extra = vec![0u8; 15];
        with_extra[10..12].copy_from_slice(&5u16.to_le_bytes());
        with_extra[12..15].copy_from_slice(&[0x00, 0x7f, 0x01]);
        assert_eq!(rows_v2_payload_offset(&with_extra), Some(15));

        let mut truncated = vec![0u8; 12];
        truncated[10..12].copy_from_slice(&5u16.to_le_bytes());
        assert_eq!(rows_v2_payload_offset(&truncated), None);

        let mut short_len = vec![0u8; 12];
        short_len[10..12].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(rows_v2_payload_offset(&short_len), None);
    }
}

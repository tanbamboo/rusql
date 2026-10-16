//! Binlog replication dump packet helpers (M174 heartbeat subset).

/// MySQL `HEARTBEAT_LOG_EVENT` type byte.
pub const HEARTBEAT_LOG_EVENT: u8 = 27;

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
}

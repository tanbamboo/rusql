//! MySQL semi-sync packet helpers (M178).
//!
//! rusql does not load `rpl_semi_sync_master` (plugin loader is M202) and does not
//! wait for replica ACKs. Callers reject the handshake with
//! [`ER_PLUGIN_IS_NOT_LOADED`] instead of ignoring ACK bytes.

/// MySQL `ER_PLUGIN_IS_NOT_LOADED`.
pub const ER_PLUGIN_IS_NOT_LOADED: u16 = 1524;

/// Source plugin a semi-sync replica expects. Not installed.
pub const RPL_SEMI_SYNC_MASTER_PLUGIN: &str = "rpl_semi_sync_master";

/// User variable the replica IO thread sets before `COM_BINLOG_DUMP`.
pub const RPL_SEMI_SYNC_SLAVE_VAR: &str = "rpl_semi_sync_slave";

/// Semi-sync magic (`kPacketMagicNum` in the MySQL semisync plugin).
pub const SEMI_SYNC_MAGIC: u8 = 0xef;

/// Replica ACK: magic, 8-byte little-endian binlog position, filename bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemiSyncAck {
    pub binlog_pos: u64,
    pub filename: String,
}

/// True when `value` is the replica's request for semi-sync (`1` / `ON` / `true` / `yes`).
pub fn semi_sync_slave_requested(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "on" | "true" | "yes"
    )
}

/// True when `payload` starts with the semi-sync magic (ACK or truncated attempt).
pub fn is_semi_sync_payload(payload: &[u8]) -> bool {
    payload.first() == Some(&SEMI_SYNC_MAGIC)
}

/// Encode a replica semi-sync ACK payload (no packet header).
pub fn encode_semi_sync_ack(binlog_pos: u64, filename: &str) -> Vec<u8> {
    let mut payload = Vec::with_capacity(1 + 8 + filename.len());
    payload.push(SEMI_SYNC_MAGIC);
    payload.extend_from_slice(&binlog_pos.to_le_bytes());
    payload.extend_from_slice(filename.as_bytes());
    payload
}

/// Parse a replica semi-sync ACK. `None` when magic is missing or the header is short.
pub fn parse_semi_sync_ack(payload: &[u8]) -> Option<SemiSyncAck> {
    if !is_semi_sync_payload(payload) || payload.len() < 9 {
        return None;
    }
    let binlog_pos = u64::from_le_bytes(payload[1..9].try_into().ok()?);
    let filename = String::from_utf8_lossy(&payload[9..]).into_owned();
    Some(SemiSyncAck {
        binlog_pos,
        filename,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semisync_ack_round_trip() {
        let raw = encode_semi_sync_ack(4_096, "binlog.000001");
        let ack = parse_semi_sync_ack(&raw).expect("ack");
        assert_eq!(ack.binlog_pos, 4_096);
        assert_eq!(ack.filename, "binlog.000001");
        assert!(is_semi_sync_payload(&raw));
    }

    #[test]
    fn semisync_ack_empty_filename() {
        let raw = encode_semi_sync_ack(4, "");
        assert_eq!(raw.len(), 9);
        let ack = parse_semi_sync_ack(&raw).expect("ack");
        assert_eq!(ack.binlog_pos, 4);
        assert!(ack.filename.is_empty());
    }

    #[test]
    fn semisync_short_magic_is_not_a_valid_ack() {
        assert!(is_semi_sync_payload(&[SEMI_SYNC_MAGIC]));
        assert!(parse_semi_sync_ack(&[SEMI_SYNC_MAGIC, 1, 2, 3]).is_none());
        assert!(!is_semi_sync_payload(&[0x00, SEMI_SYNC_MAGIC]));
        assert!(parse_semi_sync_ack(&[0x00, 0xef]).is_none());
    }

    #[test]
    fn semisync_slave_requested_values() {
        assert!(semi_sync_slave_requested("1"));
        assert!(semi_sync_slave_requested(" 1 "));
        assert!(semi_sync_slave_requested("ON"));
        assert!(semi_sync_slave_requested("true"));
        assert!(semi_sync_slave_requested("Yes"));
        assert!(!semi_sync_slave_requested("0"));
        assert!(!semi_sync_slave_requested("off"));
        assert!(!semi_sync_slave_requested("false"));
        assert!(!semi_sync_slave_requested(""));
        assert!(!semi_sync_slave_requested("10"));
    }
}

//! MySQL wire protocol implementation for rusql.

pub mod handshake;
pub mod packet;

pub use handshake::{
    encode_ok_payload, server_handshake, HandshakeConfig, HandshakeResponse, HandshakeSession,
    InitialHandshake, SERVER_CAPABILITIES,
};
pub use packet::{Packet, PacketReader, PacketWriter, MAX_PACKET_SIZE};

/// Protocol-level errors.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("{0}")]
    Message(String),
    #[error("invalid packet length: {0}")]
    InvalidLength(usize),
    #[error("packet exceeds max size")]
    PacketTooLarge,
}

impl ProtocolError {
    pub fn handshake_failed() -> Self {
        Self::Message(rusql_i18n::messages::protocol_handshake_failed())
    }

    pub fn invalid_packet() -> Self {
        Self::Message(rusql_i18n::messages::protocol_invalid_packet())
    }
}

#[cfg(test)]
mod tests {
    use super::packet::{Packet, PacketWriter};

    #[test]
    fn packet_roundtrip() {
        let payload = b"hello";
        let encoded = PacketWriter::encode(0, payload);
        let (seq, decoded) = Packet::decode(&encoded).unwrap();
        assert_eq!(seq, 0);
        assert_eq!(decoded, payload);
    }
}

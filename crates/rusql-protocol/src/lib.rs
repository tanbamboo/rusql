//! MySQL wire protocol implementation for rusql.

pub mod auth;
pub mod binary;
pub mod client_decode;
pub mod command;
pub mod framing;
pub mod handshake;
pub mod packet;
pub mod response;
pub mod stmt;

pub use auth::{
    caching_sha2_fast_scramble, encrypt_password_rsa, native_password_scramble,
    verify_auth_with_fallback, CachingSha2RsaKeys, AUTH_PLUGIN_CACHING_SHA2, AUTH_PLUGIN_NATIVE,
};
pub use binary::{
    binary_resultset_row, decode_binary_resultset_row, encode_binary_value,
    mysql_type_for_result_column, mysql_type_from_sql_type, MYSQL_TYPE_LONG, MYSQL_TYPE_LONGLONG,
    MYSQL_TYPE_VAR_STRING,
};
pub use client_decode::{is_resultset_terminator, is_resultset_terminator_for_client};
pub use command::{
    deprecate_eof_negotiated, encode_com_query_with_attributes, parse_command,
    parse_command_with_server_caps, query_attributes_negotiated, ClientCommand,
    CLIENT_DEPRECATE_EOF, CLIENT_QUERY_ATTRIBUTES, COM_QUERY, COM_QUIT, COM_STMT_CLOSE,
    COM_STMT_EXECUTE, COM_STMT_PREPARE,
};
pub use framing::{read_packet, read_packet_seq, write_packet, write_packets};
pub use handshake::{
    encode_ok_payload, server_handshake, AuthCredentials, HandshakeConfig, HandshakeResponse,
    HandshakeSession, InitialHandshake, SERVER_CAPABILITIES,
};
pub use packet::{Packet, PacketReader, PacketWriter, MAX_PACKET_SIZE};
pub use response::{
    binary_resultset, binary_resultset_for_client, err_packet, ok_packet_full, text_resultset,
    text_resultset_for_client,
};
pub use stmt::{
    encode_stmt_execute, parse_stmt_execute, stmt_eof_packet, stmt_eof_packet_for_client,
    stmt_field_definition, stmt_prepare_ok,
};

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

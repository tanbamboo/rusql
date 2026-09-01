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
pub use client_decode::{
    is_resultset_terminator, is_resultset_terminator_for_client, is_resultset_terminator_with_caps,
};
pub use command::{
    deprecate_eof_negotiated, encode_com_change_user, encode_com_field_list, encode_com_init_db,
    encode_com_query_with_attributes, encode_com_stmt_reset, encode_com_stmt_send_long_data,
    parse_command, parse_command_with_server_caps, query_attributes_negotiated,
    session_track_negotiated, ClientCommand, CLIENT_DEPRECATE_EOF, CLIENT_QUERY_ATTRIBUTES,
    CLIENT_SESSION_TRACK, COM_CHANGE_USER, COM_FIELD_LIST, COM_INIT_DB, COM_PING, COM_PROCESS_INFO,
    COM_QUERY, COM_QUIT, COM_RESET_CONNECTION, COM_STMT_CLOSE, COM_STMT_EXECUTE, COM_STMT_PREPARE,
    COM_STMT_RESET, COM_STMT_SEND_LONG_DATA,
};
pub use framing::{read_packet, read_packet_seq, write_packet, write_packets};
pub use handshake::{
    authenticate_change_user, authenticate_handshake, encode_ok_payload, exchange_handshake,
    server_handshake, AuthCredentials, AuthLookupResult, ChangeUserRequest, HandshakeConfig,
    HandshakeResponse, HandshakeSession, InitialHandshake, SERVER_CAPABILITIES,
};
pub use packet::{Packet, PacketReader, PacketWriter, MAX_PACKET_SIZE};
pub use response::{
    binary_resultset, binary_resultset_for_client, err_packet, field_list_response,
    ok_packet_for_client, ok_packet_full, text_resultset, text_resultset_for_client,
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

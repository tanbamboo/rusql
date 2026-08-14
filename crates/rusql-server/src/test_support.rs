//! Shared wire-protocol test client and ephemeral server harness.

use crate::connection::serve_connection;
use rusql_protocol::client_decode::{
    classify_query_payload, column_name_from_definition, decode_binary_row, decode_text_row,
    mysql_type_from_column_definition, QueryResponse,
};
use rusql_protocol::handshake::{HandshakeResponse, InitialHandshake};
use rusql_protocol::{
    caching_sha2_fast_scramble, deprecate_eof_negotiated, encode_com_init_db,
    encode_com_query_with_attributes, encode_stmt_execute, encrypt_password_rsa,
    is_resultset_terminator_with_caps, native_password_scramble, read_packet,
    session_track_negotiated, write_packet, HandshakeConfig, PacketWriter,
    AUTH_PLUGIN_CACHING_SHA2, CLIENT_DEPRECATE_EOF, CLIENT_QUERY_ATTRIBUTES, CLIENT_SESSION_TRACK,
    COM_PING, COM_QUERY, COM_STMT_PREPARE, SERVER_CAPABILITIES,
};
use rusql_storage::PersistentEngine;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

const CLIENT_PROTOCOL_41: u32 = 0x0000_0200;
const CLIENT_PLUGIN_AUTH: u32 = 0x0008_0000;
const CLIENT_SECURE_CONNECTION: u32 = 0x0000_8000;
const CLIENT_PLUGIN_AUTH_LENENC: u32 = 0x0020_0000;
const CLIENT_CONNECT_ATTRS: u32 = 0x0010_0000;

fn auth_response_for_plugin(password: &str, scramble: &[u8; 20], plugin: &str) -> Vec<u8> {
    if password.is_empty() {
        return vec![];
    }
    if plugin == AUTH_PLUGIN_CACHING_SHA2 {
        caching_sha2_fast_scramble(password, scramble).to_vec()
    } else {
        native_password_scramble(password, scramble).to_vec()
    }
}

pub fn temp_data_dir(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("rusql-test-{}-{}-{}", label, std::process::id(), n))
}

/// Ephemeral server bound to `127.0.0.1:0` with persistent storage.
pub struct TestServer {
    pub addr: std::net::SocketAddr,
    pub data_dir: PathBuf,
    _engine: Arc<RwLock<PersistentEngine>>,
}

impl TestServer {
    pub async fn start(label: &str) -> Self {
        Self::start_with_handshake(label, HandshakeConfig::default()).await
    }

    pub async fn start_with_handshake(label: &str, mut handshake: HandshakeConfig) -> Self {
        handshake.ensure_caching_sha2_rsa();
        let data_dir = temp_data_dir(label);
        let _ = std::fs::remove_dir_all(&data_dir);
        let engine = Arc::new(RwLock::new(PersistentEngine::open(&data_dir).unwrap()));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let eng = engine.clone();
        let cfg = handshake.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let e = eng.clone();
                let c = cfg.clone();
                tokio::spawn(async move {
                    let _ = serve_connection(&mut stream, &c, 1, e).await;
                });
            }
        });
        Self {
            addr,
            data_dir,
            _engine: engine,
        }
    }

    pub async fn connect(&self) -> WireClient {
        self.connect_as("root", "").await
    }

    /// Connect like official MySQL 8.0 CLI (`CLIENT_QUERY_ATTRIBUTES` + attribute preamble on COM_QUERY).
    pub async fn connect_like_mysql_cli(&self) -> WireClient {
        self.connect_with_caps("root", "", true).await
    }

    async fn connect_with_caps(
        &self,
        user: &str,
        password: &str,
        query_attributes: bool,
    ) -> WireClient {
        let stream = TcpStream::connect(self.addr).await.unwrap();
        let mut client = WireClient {
            stream,
            stmt_column_types: HashMap::new(),
            query_attributes,
            strict_seq: query_attributes,
        };
        client.handshake_as(user, password).await;
        client
    }

    pub async fn connect_as(&self, user: &str, password: &str) -> WireClient {
        self.connect_with_caps(user, password, false).await
    }

    pub async fn connect_rsa_as(&self, user: &str, password: &str) -> WireClient {
        let stream = TcpStream::connect(self.addr).await.unwrap();
        let mut client = WireClient {
            stream,
            stmt_column_types: HashMap::new(),
            query_attributes: false,
            strict_seq: false,
        };
        client.handshake_caching_sha2_rsa_as(user, password).await;
        client
    }

    pub async fn try_connect_as(&self, user: &str, password: &str) -> Result<(), u8> {
        let stream = TcpStream::connect(self.addr).await.unwrap();
        let mut client = WireClient {
            stream,
            stmt_column_types: HashMap::new(),
            query_attributes: false,
            strict_seq: false,
        };
        client.try_handshake_as(user, password).await
    }

    pub fn reopen_engine(&self) -> PersistentEngine {
        PersistentEngine::open(&self.data_dir).unwrap()
    }
}

pub struct WireClient {
    stream: TcpStream,
    stmt_column_types: HashMap<u32, Vec<u8>>,
    query_attributes: bool,
    strict_seq: bool,
}

impl WireClient {
    fn client_capabilities(&self) -> u32 {
        let mut caps = CLIENT_PROTOCOL_41
            | CLIENT_PLUGIN_AUTH
            | CLIENT_SECURE_CONNECTION
            | CLIENT_PLUGIN_AUTH_LENENC;
        if self.query_attributes {
            caps |= CLIENT_QUERY_ATTRIBUTES
                | CLIENT_DEPRECATE_EOF
                | CLIENT_SESSION_TRACK
                | CLIENT_CONNECT_ATTRS;
        }
        caps
    }

    fn handshake_response(
        &self,
        user: &str,
        password: &str,
        plugin: &str,
        scramble: &[u8; 20],
    ) -> HandshakeResponse {
        let mut connect_attributes = Vec::new();
        if self.query_attributes {
            connect_attributes.push(("_client_name".into(), "libmysql".into()));
            connect_attributes.push(("_client_version".into(), "8.0.33".into()));
        }
        HandshakeResponse {
            capabilities: self.client_capabilities(),
            username: user.into(),
            auth_response: auth_response_for_plugin(password, scramble, plugin),
            database: None,
            auth_plugin: Some(plugin.into()),
            connect_attributes,
        }
    }

    async fn read_packet_strict(&mut self, expected_seq: u8) -> (u8, Vec<u8>) {
        let (seq, payload) = read_packet(&mut self.stream).await.unwrap();
        if self.strict_seq {
            assert_eq!(
                seq, expected_seq,
                "unexpected response sequence (expected {expected_seq}, got {seq})"
            );
        }
        (seq, payload)
    }

    fn is_row_terminator(&self, packet: &[u8]) -> bool {
        let caps = self.client_capabilities();
        is_resultset_terminator_with_caps(
            packet,
            deprecate_eof_negotiated(caps, SERVER_CAPABILITIES),
            session_track_negotiated(caps, SERVER_CAPABILITIES),
        )
    }

    pub async fn handshake_as(&mut self, user: &str, password: &str) {
        let mut hdr = [0u8; 4];
        self.stream.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        let mut payload = vec![0u8; len];
        self.stream.read_exact(&mut payload).await.unwrap();
        let hs = InitialHandshake::decode_payload(&payload).unwrap();

        let plugin = hs.auth_plugin_name.clone();
        let response = self.handshake_response(user, password, &plugin, &hs.scramble);
        self.stream
            .write_all(&PacketWriter::encode(1, &response.encode_payload()))
            .await
            .unwrap();

        self.read_handshake_ok().await;
    }

    /// Full `caching_sha2_password` auth via RSA public-key exchange (no fast-auth scramble).
    pub async fn handshake_caching_sha2_rsa_as(&mut self, user: &str, password: &str) {
        let mut hdr = [0u8; 4];
        self.stream.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        let mut payload = vec![0u8; len];
        self.stream.read_exact(&mut payload).await.unwrap();
        let hs = InitialHandshake::decode_payload(&payload).unwrap();

        let response = HandshakeResponse {
            capabilities: self.client_capabilities(),
            username: user.into(),
            auth_response: vec![],
            database: None,
            auth_plugin: Some(AUTH_PLUGIN_CACHING_SHA2.into()),
            connect_attributes: vec![],
        };
        self.stream
            .write_all(&PacketWriter::encode(1, &response.encode_payload()))
            .await
            .unwrap();

        self.stream.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        payload.resize(len, 0);
        self.stream.read_exact(&mut payload).await.unwrap();
        assert_eq!(payload, [0x01, 0x04], "expected full-auth request");

        write_packet(&mut self.stream, 3, &[0x02]).await.unwrap();
        let (_seq, pem_pkt) = read_packet(&mut self.stream).await.unwrap();
        assert_eq!(pem_pkt.first(), Some(&0x01));
        let encrypted = encrypt_password_rsa(&pem_pkt[1..], password, &hs.scramble).unwrap();
        write_packet(&mut self.stream, 5, &encrypted).await.unwrap();
        self.read_handshake_ok().await;
    }

    async fn read_handshake_ok(&mut self) {
        let mut hdr = [0u8; 4];
        loop {
            self.stream.read_exact(&mut hdr).await.unwrap();
            let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
            let mut payload = vec![0u8; len];
            self.stream.read_exact(&mut payload).await.unwrap();
            match payload.first() {
                Some(0x00) => return,
                Some(0x01) if payload.get(1) == Some(&0x03) => continue,
                Some(0xFF) => panic!("handshake ERR: {payload:?}"),
                _ => panic!("unexpected handshake packet: {payload:?}"),
            }
        }
    }

    pub async fn try_handshake_as(&mut self, user: &str, password: &str) -> Result<(), u8> {
        let mut hdr = [0u8; 4];
        self.stream.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        let mut payload = vec![0u8; len];
        self.stream.read_exact(&mut payload).await.unwrap();
        let hs = InitialHandshake::decode_payload(&payload).unwrap();

        let plugin = hs.auth_plugin_name.clone();
        let response = self.handshake_response(user, password, &plugin, &hs.scramble);
        self.stream
            .write_all(&PacketWriter::encode(1, &response.encode_payload()))
            .await
            .unwrap();

        let mut hdr = [0u8; 4];
        self.stream.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        let mut payload = vec![0u8; len];
        self.stream.read_exact(&mut payload).await.unwrap();
        if payload[0] == 0x00 {
            Ok(())
        } else if payload.first() == Some(&0x01) && payload.get(1) == Some(&0x03) {
            self.stream.read_exact(&mut hdr).await.unwrap();
            let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
            payload.resize(len, 0);
            self.stream.read_exact(&mut payload).await.unwrap();
            if payload[0] == 0x00 {
                Ok(())
            } else {
                Err(payload[0])
            }
        } else {
            Err(payload[0])
        }
    }

    pub async fn query_plain(&mut self, sql: &str) -> QueryResponse {
        let mut cmd = vec![COM_QUERY];
        cmd.extend_from_slice(sql.as_bytes());
        write_packet(&mut self.stream, 0, &cmd).await.unwrap();
        self.read_query_response(1).await
    }

    pub async fn query(&mut self, sql: &str) -> QueryResponse {
        let cmd = if self.query_attributes {
            encode_com_query_with_attributes(sql)
        } else {
            let mut cmd = vec![COM_QUERY];
            cmd.extend_from_slice(sql.as_bytes());
            cmd
        };
        write_packet(&mut self.stream, 0, &cmd).await.unwrap();
        self.read_query_response(1).await
    }

    pub async fn stmt_prepare(&mut self, sql: &str) -> u32 {
        let mut cmd = vec![COM_STMT_PREPARE];
        cmd.extend_from_slice(sql.as_bytes());
        write_packet(&mut self.stream, 0, &cmd).await.unwrap();
        let (_seq, payload) = read_packet(&mut self.stream).await.unwrap();
        assert_eq!(payload[0], 0, "prepare failed: {:?}", payload);
        let stmt_id = u32::from_le_bytes(payload[1..5].try_into().unwrap());
        let num_columns = u16::from_le_bytes(payload[5..7].try_into().unwrap());
        let num_params = u16::from_le_bytes(payload[7..9].try_into().unwrap());
        for _ in 0..num_params {
            let _ = read_packet(&mut self.stream).await.unwrap();
        }
        if num_params > 0 {
            let _ = read_packet(&mut self.stream).await.unwrap();
        }
        let mut col_types = Vec::with_capacity(num_columns as usize);
        for _ in 0..num_columns {
            let (_s, def) = read_packet(&mut self.stream).await.unwrap();
            col_types.push(mysql_type_from_column_definition(&def).unwrap());
        }
        if num_columns > 0 {
            let _ = read_packet(&mut self.stream).await.unwrap();
        }
        if !col_types.is_empty() {
            self.stmt_column_types.insert(stmt_id, col_types);
        }
        stmt_id
    }

    pub async fn stmt_execute(&mut self, stmt_id: u32, params: &[Option<String>]) -> QueryResponse {
        let cmd = encode_stmt_execute(stmt_id, params);
        write_packet(&mut self.stream, 0, &cmd).await.unwrap();
        self.read_stmt_execute_response(stmt_id).await
    }

    pub async fn init_db(&mut self, database: &str) -> QueryResponse {
        let cmd = encode_com_init_db(database);
        write_packet(&mut self.stream, 0, &cmd).await.unwrap();
        let (_seq, payload) = read_packet(&mut self.stream).await.unwrap();
        classify_query_payload(&payload).unwrap()
    }

    pub async fn ping(&mut self) -> QueryResponse {
        write_packet(&mut self.stream, 0, &[COM_PING])
            .await
            .unwrap();
        let (_seq, payload) = read_packet(&mut self.stream).await.unwrap();
        classify_query_payload(&payload).unwrap()
    }

    pub async fn quit(&mut self) {
        write_packet(&mut self.stream, 0, &[0x01]).await.unwrap();
    }

    async fn read_stmt_execute_response(&mut self, stmt_id: u32) -> QueryResponse {
        let prepared_types = self
            .stmt_column_types
            .get(&stmt_id)
            .cloned()
            .unwrap_or_default();
        let (_seq, first) = read_packet(&mut self.stream).await.unwrap();
        let response = classify_query_payload(&first).unwrap();
        match response {
            QueryResponse::Rows { .. } => {
                let col_count =
                    rusql_protocol::client_decode::read_lenenc_int(&first, &mut 0) as usize;
                let mut columns = Vec::with_capacity(col_count);
                let mut col_types = Vec::with_capacity(col_count);
                for _ in 0..col_count {
                    let (_s, def) = read_packet(&mut self.stream).await.unwrap();
                    columns.push(column_name_from_definition(&def).unwrap());
                    col_types.push(mysql_type_from_column_definition(&def).unwrap());
                }
                let wire_types = if prepared_types.len() == col_count {
                    prepared_types
                } else {
                    col_types
                };
                let mut rows = Vec::new();
                loop {
                    let (_s, packet) = read_packet(&mut self.stream).await.unwrap();
                    if self.is_row_terminator(&packet) {
                        break;
                    }
                    if packet.first() == Some(&0x00) {
                        rows.push(decode_binary_row(&wire_types, &packet).unwrap());
                    } else {
                        rows.push(decode_text_row(&packet).unwrap());
                    }
                }
                QueryResponse::Rows { columns, rows }
            }
            other => other,
        }
    }

    async fn read_query_response(&mut self, mut seq: u8) -> QueryResponse {
        let (_s, first) = self.read_packet_strict(seq).await;
        seq = seq.wrapping_add(1);
        let response = classify_query_payload(&first).unwrap();
        match response {
            QueryResponse::Rows {
                columns: _,
                rows: _,
            } => {
                let col_count =
                    rusql_protocol::client_decode::read_lenenc_int(&first, &mut 0) as usize;
                let mut columns = Vec::with_capacity(col_count);
                for _ in 0..col_count {
                    let (_s, def) = self.read_packet_strict(seq).await;
                    seq = seq.wrapping_add(1);
                    columns.push(column_name_from_definition(&def).unwrap());
                }
                let mut rows = Vec::new();
                loop {
                    let (_s, packet) = self.read_packet_strict(seq).await;
                    if self.is_row_terminator(&packet) {
                        break;
                    }
                    seq = seq.wrapping_add(1);
                    rows.push(decode_text_row(&packet).unwrap());
                }
                QueryResponse::Rows { columns, rows }
            }
            other => other,
        }
    }
}

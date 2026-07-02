//! Shared wire-protocol test client and ephemeral server harness.

use crate::connection::serve_connection;
use rusql_protocol::client_decode::{
    classify_query_payload, column_name_from_definition, decode_binary_row, decode_text_row,
    mysql_type_from_column_definition, QueryResponse,
};
use rusql_protocol::handshake::{HandshakeResponse, InitialHandshake};
use rusql_protocol::{
    caching_sha2_fast_scramble, encode_stmt_execute, encrypt_password_rsa,
    native_password_scramble, read_packet, write_packet, HandshakeConfig, PacketWriter,
    AUTH_PLUGIN_CACHING_SHA2, COM_QUERY, COM_STMT_PREPARE,
};
use rusql_storage::PersistentEngine;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

const CLIENT_PROTOCOL_41: u32 = 0x0000_0200;
const CLIENT_PLUGIN_AUTH: u32 = 0x0008_0000;
const CLIENT_SECURE_CONNECTION: u32 = 0x0000_8000;
const CLIENT_PLUGIN_AUTH_LENENC: u32 = 0x0020_0000;

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
    _engine: Arc<Mutex<PersistentEngine>>,
}

impl TestServer {
    pub async fn start(label: &str) -> Self {
        Self::start_with_handshake(label, HandshakeConfig::default()).await
    }

    pub async fn start_with_handshake(label: &str, mut handshake: HandshakeConfig) -> Self {
        handshake.ensure_caching_sha2_rsa();
        let data_dir = temp_data_dir(label);
        let _ = std::fs::remove_dir_all(&data_dir);
        let engine = Arc::new(Mutex::new(PersistentEngine::open(&data_dir).unwrap()));
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

    pub async fn connect_rsa_as(&self, user: &str, password: &str) -> WireClient {
        let stream = TcpStream::connect(self.addr).await.unwrap();
        let mut client = WireClient {
            stream,
            stmt_column_types: HashMap::new(),
        };
        client.handshake_caching_sha2_rsa_as(user, password).await;
        client
    }

    pub async fn connect_as(&self, user: &str, password: &str) -> WireClient {
        let stream = TcpStream::connect(self.addr).await.unwrap();
        let mut client = WireClient {
            stream,
            stmt_column_types: HashMap::new(),
        };
        client.handshake_as(user, password).await;
        client
    }

    pub async fn try_connect_as(&self, user: &str, password: &str) -> Result<(), u8> {
        let stream = TcpStream::connect(self.addr).await.unwrap();
        let mut client = WireClient {
            stream,
            stmt_column_types: HashMap::new(),
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
}

impl WireClient {
    pub async fn handshake_as(&mut self, user: &str, password: &str) {
        let mut hdr = [0u8; 4];
        self.stream.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        let mut payload = vec![0u8; len];
        self.stream.read_exact(&mut payload).await.unwrap();
        let hs = InitialHandshake::decode_payload(&payload).unwrap();

        let plugin = hs.auth_plugin_name.clone();
        let auth_response = auth_response_for_plugin(password, &hs.scramble, &plugin);

        let response = HandshakeResponse {
            capabilities: CLIENT_PROTOCOL_41
                | CLIENT_PLUGIN_AUTH
                | CLIENT_SECURE_CONNECTION
                | CLIENT_PLUGIN_AUTH_LENENC,
            username: user.into(),
            auth_response,
            database: None,
            auth_plugin: Some(plugin),
        };
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
            capabilities: CLIENT_PROTOCOL_41
                | CLIENT_PLUGIN_AUTH
                | CLIENT_SECURE_CONNECTION
                | CLIENT_PLUGIN_AUTH_LENENC,
            username: user.into(),
            auth_response: vec![],
            database: None,
            auth_plugin: Some(AUTH_PLUGIN_CACHING_SHA2.into()),
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
        let auth_response = auth_response_for_plugin(password, &hs.scramble, &plugin);

        let response = HandshakeResponse {
            capabilities: CLIENT_PROTOCOL_41
                | CLIENT_PLUGIN_AUTH
                | CLIENT_SECURE_CONNECTION
                | CLIENT_PLUGIN_AUTH_LENENC,
            username: user.into(),
            auth_response,
            database: None,
            auth_plugin: Some(plugin),
        };
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

    pub async fn query(&mut self, sql: &str) -> QueryResponse {
        let mut cmd = vec![COM_QUERY];
        cmd.extend_from_slice(sql.as_bytes());
        write_packet(&mut self.stream, 0, &cmd).await.unwrap();
        self.read_query_response().await
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
                    if !packet.is_empty() && packet[0] == 0xFE {
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

    async fn read_query_response(&mut self) -> QueryResponse {
        let (_seq, first) = read_packet(&mut self.stream).await.unwrap();
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
                    let (_s, def) = read_packet(&mut self.stream).await.unwrap();
                    columns.push(column_name_from_definition(&def).unwrap());
                }
                let mut rows = Vec::new();
                loop {
                    let (_s, packet) = read_packet(&mut self.stream).await.unwrap();
                    if !packet.is_empty() && packet[0] == 0xFE {
                        break;
                    }
                    rows.push(decode_text_row(&packet).unwrap());
                }
                QueryResponse::Rows { columns, rows }
            }
            other => other,
        }
    }
}

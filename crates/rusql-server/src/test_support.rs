//! Shared wire-protocol test client and ephemeral server harness.

use crate::connection::serve_connection;
use rusql_protocol::client_decode::{
    classify_query_payload, column_name_from_definition, decode_text_row, QueryResponse,
};
use rusql_protocol::handshake::{HandshakeResponse, InitialHandshake};
use rusql_protocol::HandshakeConfig;
use rusql_protocol::{read_packet, write_packet, PacketWriter, COM_QUERY};
use rusql_storage::PersistentEngine;
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
        let data_dir = temp_data_dir(label);
        let _ = std::fs::remove_dir_all(&data_dir);
        let engine = Arc::new(Mutex::new(PersistentEngine::open(&data_dir).unwrap()));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let eng = engine.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let e = eng.clone();
                tokio::spawn(async move {
                    let _ = serve_connection(&mut stream, &HandshakeConfig::default(), 1, e).await;
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
        let stream = TcpStream::connect(self.addr).await.unwrap();
        let mut client = WireClient { stream };
        client.handshake().await;
        client
    }

    pub fn reopen_engine(&self) -> PersistentEngine {
        PersistentEngine::open(&self.data_dir).unwrap()
    }
}

pub struct WireClient {
    stream: TcpStream,
}

impl WireClient {
    pub async fn handshake(&mut self) {
        let mut hdr = [0u8; 4];
        self.stream.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        let mut payload = vec![0u8; len];
        self.stream.read_exact(&mut payload).await.unwrap();
        InitialHandshake::decode_payload(&payload).unwrap();

        let response = HandshakeResponse {
            capabilities: CLIENT_PROTOCOL_41
                | CLIENT_PLUGIN_AUTH
                | CLIENT_SECURE_CONNECTION
                | CLIENT_PLUGIN_AUTH_LENENC,
            username: "root".into(),
            auth_response: vec![0],
            database: None,
            auth_plugin: Some("mysql_native_password".into()),
        };
        self.stream
            .write_all(&PacketWriter::encode(1, &response.encode_payload()))
            .await
            .unwrap();

        self.stream.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        payload.resize(len, 0);
        self.stream.read_exact(&mut payload).await.unwrap();
        assert_eq!(payload[0], 0x00);
    }

    pub async fn query(&mut self, sql: &str) -> QueryResponse {
        let mut cmd = vec![COM_QUERY];
        cmd.extend_from_slice(sql.as_bytes());
        write_packet(&mut self.stream, 0, &cmd).await.unwrap();
        self.read_query_response().await
    }

    pub async fn quit(&mut self) {
        write_packet(&mut self.stream, 0, &[0x01]).await.unwrap();
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

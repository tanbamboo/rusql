//! MySQL TLS upgrade (`CLIENT_SSL`) using rustls. No client-certificate request (M184).

use rusql_protocol::handshake::{
    decode_client_hello, ensure_supported_auth_plugin, write_server_greeting, ClientHello,
    HandshakeConfig, HandshakeResponse, InitialHandshake, ER_HANDSHAKE_ERROR,
    ER_SECURE_TRANSPORT_REQUIRED,
};
use rusql_protocol::{err_packet, read_packet_seq, write_packets, ProtocolError};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_rustls::TlsAcceptor;

pub use tokio_rustls::TlsAcceptor as ServerTlsAcceptor;

/// Plain or TLS byte stream for the rest of the session.
pub enum SessionIo<S> {
    Plain(S),
    Tls(Box<tokio_rustls::server::TlsStream<S>>),
}

impl<S> SessionIo<S> {
    pub fn is_tls(&self) -> bool {
        matches!(self, SessionIo::Tls(_))
    }
}

impl<S> AsyncRead for SessionIo<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            SessionIo::Plain(inner) => Pin::new(inner).poll_read(cx, buf),
            SessionIo::Tls(inner) => Pin::new(inner.as_mut()).poll_read(cx, buf),
        }
    }
}

impl<S> AsyncWrite for SessionIo<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            SessionIo::Plain(inner) => Pin::new(inner).poll_write(cx, buf),
            SessionIo::Tls(inner) => Pin::new(inner.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            SessionIo::Plain(inner) => Pin::new(inner).poll_flush(cx),
            SessionIo::Tls(inner) => Pin::new(inner.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            SessionIo::Plain(inner) => Pin::new(inner).poll_shutdown(cx),
            SessionIo::Tls(inner) => Pin::new(inner.as_mut()).poll_shutdown(cx),
        }
    }
}

/// Greeting plus the client's login packet, on plaintext or TLS.
pub struct AcceptedConnection<S> {
    pub io: SessionIo<S>,
    pub handshake: InitialHandshake,
    pub response: HandshakeResponse,
    pub auth_seq: u8,
}

fn ring_provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Load a PEM certificate chain and private key. Does not log key bytes.
pub fn load_tls_acceptor(cert_path: &Path, key_path: &Path) -> anyhow::Result<TlsAcceptor> {
    let cert_bytes = std::fs::read(cert_path).map_err(|_| {
        anyhow::anyhow!(rusql_i18n::messages::server_ssl_file_unreadable(
            &cert_path.display().to_string(),
        ))
    })?;
    let key_bytes = std::fs::read(key_path).map_err(|_| {
        anyhow::anyhow!(rusql_i18n::messages::server_ssl_file_unreadable(
            &key_path.display().to_string(),
        ))
    })?;
    let mut cert_reader = std::io::Cursor::new(cert_bytes);
    let certs: Vec<_> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| anyhow::anyhow!(rusql_i18n::messages::server_ssl_material_invalid()))?;
    if certs.is_empty() {
        return Err(anyhow::anyhow!(
            rusql_i18n::messages::server_ssl_material_invalid()
        ));
    }
    let mut key_reader = std::io::Cursor::new(key_bytes);
    let key = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|_| anyhow::anyhow!(rusql_i18n::messages::server_ssl_material_invalid()))?
        .ok_or_else(|| anyhow::anyhow!(rusql_i18n::messages::server_ssl_material_invalid()))?;
    let config = rustls::ServerConfig::builder_with_provider(ring_provider())
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .map_err(|_| anyhow::anyhow!(rusql_i18n::messages::server_ssl_material_invalid()))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|_| anyhow::anyhow!(rusql_i18n::messages::server_ssl_material_invalid()))?;
    Ok(TlsAcceptor::from(Arc::new(config)))
}

/// Both paths, or neither. A single flag is a startup error.
pub fn resolve_tls_acceptor(
    cert: Option<&Path>,
    key: Option<&Path>,
) -> anyhow::Result<Option<TlsAcceptor>> {
    match (cert, key) {
        (None, None) => Ok(None),
        (Some(cert), Some(key)) => Ok(Some(load_tls_acceptor(cert, key)?)),
        _ => anyhow::bail!(rusql_i18n::messages::server_ssl_flags_incomplete()),
    }
}

/// Write the greeting, optionally upgrade to TLS, then read the login packet.
pub async fn accept_mysql_connection<S>(
    mut stream: S,
    config: &HandshakeConfig,
    connection_id: u32,
    tls: Option<TlsAcceptor>,
) -> Result<AcceptedConnection<S>, ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut config = config.clone();
    config.tls_enabled = tls.is_some();
    let handshake = write_server_greeting(&mut stream, &config, connection_id).await?;
    let first = read_packet_seq(&mut stream, 1).await?;
    match decode_client_hello(&first)? {
        ClientHello::SslRequest => {
            let Some(acceptor) = tls else {
                let err = err_packet(
                    ER_HANDSHAKE_ERROR,
                    &rusql_i18n::messages::protocol_bad_handshake(),
                );
                let _ = write_packets(&mut stream, 2, &[err]).await;
                return Err(ProtocolError::Message(
                    rusql_i18n::messages::protocol_bad_handshake(),
                ));
            };
            let mut tls_stream = match acceptor.accept(stream).await {
                Ok(upgraded) => upgraded,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        "{}",
                        rusql_i18n::messages::protocol_tls_handshake_failed()
                    );
                    return Err(ProtocolError::Message(
                        rusql_i18n::messages::protocol_tls_handshake_failed(),
                    ));
                }
            };
            let payload = read_packet_seq(&mut tls_stream, 2).await?;
            let response = match decode_client_hello(&payload)? {
                ClientHello::Handshake(response) => response,
                ClientHello::SslRequest => return Err(ProtocolError::handshake_failed()),
            };
            ensure_supported_auth_plugin(&mut tls_stream, &config, &response, 3).await?;
            Ok(AcceptedConnection {
                io: SessionIo::Tls(Box::new(tls_stream)),
                handshake,
                response,
                auth_seq: 3,
            })
        }
        ClientHello::Handshake(response) => {
            if config.require_secure_transport {
                let err = err_packet(
                    ER_SECURE_TRANSPORT_REQUIRED,
                    &rusql_i18n::messages::protocol_secure_transport_required(),
                );
                let _ = write_packets(&mut stream, 2, &[err]).await;
                return Err(ProtocolError::Message(
                    rusql_i18n::messages::protocol_secure_transport_required(),
                ));
            }
            ensure_supported_auth_plugin(&mut stream, &config, &response, 2).await?;
            Ok(AcceptedConnection {
                io: SessionIo::Plain(stream),
                handshake,
                response,
                auth_seq: 2,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{temp_data_dir, TestServer};
    use rusql_protocol::client_decode::QueryResponse;
    use rusql_protocol::handshake::{
        capability_flags_from_initial_handshake, HandshakeConfig, HandshakeResponse, CLIENT_SSL,
        ER_HANDSHAKE_ERROR, ER_SECURE_TRANSPORT_REQUIRED,
    };
    use rusql_protocol::{PacketWriter, AUTH_PLUGIN_CACHING_SHA2};
    use std::path::PathBuf;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    const CLIENT_PROTOCOL_41: u32 = 0x0000_0200;
    const CLIENT_PLUGIN_AUTH: u32 = 0x0008_0000;
    const CLIENT_SECURE_CONNECTION: u32 = 0x0000_8000;
    const CLIENT_PLUGIN_AUTH_LENENC: u32 = 0x0020_0000;

    fn greeting_offers_ssl(payload: &[u8]) -> bool {
        capability_flags_from_initial_handshake(payload)
            .map(|caps| caps & CLIENT_SSL != 0)
            .unwrap_or(false)
    }

    fn write_ephemeral_cert(dir: &std::path::Path) -> (PathBuf, PathBuf, Vec<u8>) {
        let key_pair = rcgen::KeyPair::generate().expect("key");
        let mut params =
            rcgen::CertificateParams::new(vec!["localhost".to_string()]).expect("params");
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "localhost");
        let cert = params.self_signed(&key_pair).expect("cert");
        let cert_path = dir.join("server.crt");
        let key_path = dir.join("server.key");
        std::fs::write(&cert_path, cert.pem()).unwrap();
        std::fs::write(&key_path, key_pair.serialize_pem()).unwrap();
        let der = cert.der().as_ref().to_vec();
        (cert_path, key_path, der)
    }

    async fn read_frame<S>(stream: &mut S) -> (u8, Vec<u8>)
    where
        S: AsyncRead + Unpin,
    {
        let mut hdr = [0u8; 4];
        stream.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await.unwrap();
        (hdr[3], payload)
    }

    fn err_code(payload: &[u8]) -> u16 {
        assert_eq!(
            payload.first(),
            Some(&0xFF),
            "expected ERR, got {payload:?}"
        );
        u16::from_le_bytes(payload[1..3].try_into().unwrap())
    }

    fn login_caps() -> u32 {
        CLIENT_PROTOCOL_41
            | CLIENT_PLUGIN_AUTH
            | CLIENT_SECURE_CONNECTION
            | CLIENT_PLUGIN_AUTH_LENENC
            | CLIENT_SSL
    }

    fn ssl_request_bytes() -> Vec<u8> {
        let mut payload = vec![0u8; 32];
        payload[..4].copy_from_slice(&login_caps().to_le_bytes());
        payload[8] = 45;
        payload
    }

    async fn assert_offers_ssl(addr: std::net::SocketAddr, offered: bool) {
        let mut tcp = TcpStream::connect(addr).await.unwrap();
        let (_seq, greeting) = read_frame(&mut tcp).await;
        assert_eq!(greeting_offers_ssl(&greeting), offered);
    }

    async fn tls_select_one(addr: std::net::SocketAddr, der: &[u8]) {
        let mut tcp = TcpStream::connect(addr).await.unwrap();
        let (_seq, greeting) = read_frame(&mut tcp).await;
        assert!(greeting_offers_ssl(&greeting));
        let hs = rusql_protocol::handshake::InitialHandshake::decode_payload(&greeting).unwrap();
        tcp.write_all(&PacketWriter::encode(1, &ssl_request_bytes()))
            .await
            .unwrap();

        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from(der.to_vec()))
            .unwrap();
        let config = rustls::ClientConfig::builder_with_provider(ring_provider())
            .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
        let mut tls = connector.connect(name, tcp).await.expect("tls handshake");
        let presented = tls
            .get_ref()
            .1
            .peer_certificates()
            .expect("server certificate");
        assert_eq!(
            presented[0].as_ref(),
            der,
            "server must present the configured cert"
        );

        let response = HandshakeResponse {
            capabilities: login_caps(),
            username: "root".into(),
            auth_response: vec![],
            database: None,
            auth_plugin: Some(AUTH_PLUGIN_CACHING_SHA2.into()),
            connect_attributes: vec![],
        };
        let _ = hs;
        tls.write_all(&PacketWriter::encode(2, &response.encode_payload()))
            .await
            .unwrap();
        let mut saw_ok = false;
        for _ in 0..4 {
            let (_seq, payload) = read_frame(&mut tls).await;
            if payload.first() == Some(&0xFF) {
                panic!("tls login ERR: {payload:?}");
            }
            if payload.first() == Some(&0x00) {
                saw_ok = true;
                break;
            }
        }
        assert!(saw_ok, "TLS login did not return OK");

        let mut query = vec![0x03];
        query.extend_from_slice(b"SELECT 1");
        tls.write_all(&PacketWriter::encode(0, &query))
            .await
            .unwrap();
        let (_seq, payload) = read_frame(&mut tls).await;
        assert_ne!(
            payload.first(),
            Some(&0xFF),
            "SELECT 1 over TLS failed: {payload:?}"
        );
    }

    fn oracle_mysql_enabled() -> bool {
        if std::env::var_os("CI").is_some() && std::env::var_os("RUSQL_ORACLE_MYSQL").is_none() {
            return false;
        }
        std::process::Command::new("mysql")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn mysql_ssl(port: u16, mode: &str) -> std::process::Output {
        let port = port.to_string();
        let mode_arg = format!("--ssl-mode={mode}");
        std::process::Command::new("mysql")
            .args([
                "-h",
                "127.0.0.1",
                "-P",
                &port,
                "-u",
                "root",
                "--protocol=TCP",
                &mode_arg,
                "--connect-timeout=5",
                "-e",
                "SELECT 1",
            ])
            .output()
            .expect("spawn mysql")
    }

    #[test]
    fn tls_listen_requires_both_flags() {
        let err = resolve_tls_acceptor(Some(Path::new("only-cert.pem")), None)
            .err()
            .expect("incomplete flags");
        assert!(err.to_string().contains("--ssl-cert"));
        assert!(resolve_tls_acceptor(None, None).unwrap().is_none());
    }

    #[test]
    fn tls_listen_rejects_invalid_pem() {
        let dir = temp_data_dir("tls_bad_pem");
        std::fs::create_dir_all(&dir).unwrap();
        let cert = dir.join("c.pem");
        let key = dir.join("k.pem");
        std::fs::write(&cert, b"not-a-certificate").unwrap();
        std::fs::write(&key, b"not-a-key").unwrap();
        let err = load_tls_acceptor(&cert, &key)
            .err()
            .expect("bad pem")
            .to_string();
        assert!(
            err.contains("PEM")
                || err.contains("PEM".to_lowercase().as_str())
                || err.contains("证书")
        );
        assert!(!err.contains("not-a-key"));
        let missing = dir.join("missing.pem");
        let unread = load_tls_acceptor(&missing, &key)
            .err()
            .expect("missing file")
            .to_string();
        assert!(unread.contains("missing.pem"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn tls_listen() {
        let dir = temp_data_dir("tls_listen_cert");
        std::fs::create_dir_all(&dir).unwrap();
        let (cert_path, key_path, der) = write_ephemeral_cert(&dir);
        let acceptor = load_tls_acceptor(&cert_path, &key_path).unwrap();
        let cfg = HandshakeConfig {
            tls_enabled: true,
            ..HandshakeConfig::default()
        };
        let server = TestServer::start_with_tls("tls_listen", cfg.clone(), Some(acceptor)).await;

        assert_offers_ssl(server.addr, true).await;
        let mut plain = server.connect().await;
        if let QueryResponse::Err { code, message } = plain.query("SELECT 1").await {
            panic!("plaintext on TLS server: {code} {message}");
        }
        plain.quit().await;
        tls_select_one(server.addr, &der).await;

        let plain_server = TestServer::start("tls_listen_plain").await;
        assert_offers_ssl(plain_server.addr, false).await;
        let mut still_plain = plain_server.connect().await;
        if let QueryResponse::Err { code, message } = still_plain.query("SELECT 1").await {
            panic!("default plaintext: {code} {message}");
        }
        still_plain.quit().await;

        let mut raw = TcpStream::connect(plain_server.addr).await.unwrap();
        let _ = read_frame(&mut raw).await;
        raw.write_all(&PacketWriter::encode(1, &ssl_request_bytes()))
            .await
            .unwrap();
        let (_seq, payload) = read_frame(&mut raw).await;
        assert_eq!(err_code(&payload), ER_HANDSHAKE_ERROR);

        let secure_cfg = HandshakeConfig {
            require_secure_transport: true,
            ..cfg
        };
        let secure_acceptor = load_tls_acceptor(&cert_path, &key_path).unwrap();
        let secure =
            TestServer::start_with_tls("tls_listen_required", secure_cfg, Some(secure_acceptor))
                .await;
        let mut blocked = TcpStream::connect(secure.addr).await.unwrap();
        let (_seq, greeting) = read_frame(&mut blocked).await;
        assert!(greeting_offers_ssl(&greeting));
        let hs = rusql_protocol::handshake::InitialHandshake::decode_payload(&greeting).unwrap();
        let response = HandshakeResponse {
            capabilities: login_caps(),
            username: "root".into(),
            auth_response: vec![],
            database: None,
            auth_plugin: Some(AUTH_PLUGIN_CACHING_SHA2.into()),
            connect_attributes: vec![],
        };
        let _ = hs;
        blocked
            .write_all(&PacketWriter::encode(1, &response.encode_payload()))
            .await
            .unwrap();
        let (_seq, payload) = read_frame(&mut blocked).await;
        assert_eq!(err_code(&payload), ER_SECURE_TRANSPORT_REQUIRED);
        tls_select_one(secure.addr, &der).await;

        if oracle_mysql_enabled() {
            let required_plain = mysql_ssl(plain_server.addr.port(), "REQUIRED");
            assert!(
                !required_plain.status.success(),
                "REQUIRED against a plaintext server must fail: {}",
                String::from_utf8_lossy(&required_plain.stdout)
            );
            let required_tls = mysql_ssl(server.addr.port(), "REQUIRED");
            assert!(
                required_tls.status.success(),
                "REQUIRED against TLS server failed: {}",
                String::from_utf8_lossy(&required_tls.stderr)
            );
            let disabled = mysql_ssl(server.addr.port(), "DISABLED");
            assert!(
                disabled.status.success(),
                "DISABLED against TLS server failed: {}",
                String::from_utf8_lossy(&disabled.stderr)
            );
        }

        let _ = key_path;
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&server.data_dir);
        let _ = std::fs::remove_dir_all(&plain_server.data_dir);
        let _ = std::fs::remove_dir_all(&secure.data_dir);
    }

    async fn plaintext_require_ssl_errno(
        addr: std::net::SocketAddr,
        user: &str,
        password: &str,
    ) -> u16 {
        let mut tcp = TcpStream::connect(addr).await.unwrap();
        let (_seq, greeting) = read_frame(&mut tcp).await;
        let hs = rusql_protocol::handshake::InitialHandshake::decode_payload(&greeting).unwrap();
        let auth = rusql_protocol::caching_sha2_fast_scramble(password, &hs.scramble);
        let response = HandshakeResponse {
            capabilities: login_caps(),
            username: user.into(),
            auth_response: auth.to_vec(),
            database: None,
            auth_plugin: Some(AUTH_PLUGIN_CACHING_SHA2.into()),
            connect_attributes: vec![],
        };
        tcp.write_all(&PacketWriter::encode(1, &response.encode_payload()))
            .await
            .unwrap();
        let (_seq, payload) = read_frame(&mut tcp).await;
        assert_eq!(
            payload.first(),
            Some(&0xFF),
            "REQUIRE SSL plaintext login must be ERR, got {payload:?}"
        );
        let code = err_code(&payload);
        assert_ne!(code, ER_SECURE_TRANSPORT_REQUIRED);
        code
    }

    async fn tls_require_ssl_select(
        addr: std::net::SocketAddr,
        der: &[u8],
        user: &str,
        password: &str,
    ) {
        let mut tcp = TcpStream::connect(addr).await.unwrap();
        let (_seq, greeting) = read_frame(&mut tcp).await;
        let hs = rusql_protocol::handshake::InitialHandshake::decode_payload(&greeting).unwrap();
        tcp.write_all(&PacketWriter::encode(1, &ssl_request_bytes()))
            .await
            .unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from(der.to_vec()))
            .unwrap();
        let config = rustls::ClientConfig::builder_with_provider(ring_provider())
            .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
        let mut tls = connector.connect(name, tcp).await.expect("tls handshake");
        let auth = rusql_protocol::caching_sha2_fast_scramble(password, &hs.scramble);
        let response = HandshakeResponse {
            capabilities: login_caps(),
            username: user.into(),
            auth_response: auth.to_vec(),
            database: None,
            auth_plugin: Some(AUTH_PLUGIN_CACHING_SHA2.into()),
            connect_attributes: vec![],
        };
        tls.write_all(&PacketWriter::encode(2, &response.encode_payload()))
            .await
            .unwrap();
        let mut saw_ok = false;
        for _ in 0..4 {
            let (_seq, payload) = read_frame(&mut tls).await;
            if payload.first() == Some(&0xFF) {
                panic!("TLS REQUIRE SSL login ERR {}", err_code(&payload));
            }
            if payload.first() == Some(&0x00) {
                saw_ok = true;
                break;
            }
        }
        assert!(saw_ok, "TLS login did not return OK");
        let mut query = vec![0x03];
        query.extend_from_slice(b"SELECT 1");
        tls.write_all(&PacketWriter::encode(0, &query))
            .await
            .unwrap();
        let (_seq, payload) = read_frame(&mut tls).await;
        assert_ne!(
            payload.first(),
            Some(&0xFF),
            "SELECT 1 over TLS: {payload:?}"
        );
    }

    /// M184: REQUIRE SSL rejects plaintext (errno 1045) and accepts the M181 TLS session.
    #[tokio::test]
    async fn require_ssl_plaintext_denied_tls_ok() {
        let dir = temp_data_dir("require_ssl_cert");
        std::fs::create_dir_all(&dir).unwrap();
        let (cert_path, key_path, der) = write_ephemeral_cert(&dir);
        let acceptor = load_tls_acceptor(&cert_path, &key_path).unwrap();
        let server =
            TestServer::start_with_tls("require_ssl", HandshakeConfig::default(), Some(acceptor))
                .await;
        let mut admin = server.connect().await;
        for sql in [
            "CREATE USER 'm184ssl'@'%' IDENTIFIED BY 'secret' REQUIRE SSL",
            "CREATE USER 'm184plain'@'%' IDENTIFIED BY 'secret'",
        ] {
            assert!(
                matches!(admin.query(sql).await, QueryResponse::Ok { .. }),
                "{sql}"
            );
        }
        match admin.query("SHOW CREATE USER 'm184ssl'@'%'").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][0].contains("REQUIRE SSL"),
                    "show create: {}",
                    rows[0][0]
                );
                assert!(!rows[0][0].contains("secret"));
            }
            other => panic!("SHOW CREATE USER: {other:?}"),
        }
        match admin.query("SHOW CREATE USER 'm184plain'@'%'").await {
            QueryResponse::Rows { rows, .. } => assert!(!rows[0][0].contains("REQUIRE SSL")),
            other => panic!("plain show create: {other:?}"),
        }
        match admin
            .query("CREATE USER 'm184x'@'%' IDENTIFIED BY 'secret' REQUIRE X509")
            .await
        {
            QueryResponse::Err { code: 1064, .. } => {}
            other => panic!("REQUIRE X509 must be rejected, got {other:?}"),
        }
        match admin
            .query("ALTER USER 'm184plain'@'%' REQUIRE ISSUER '/CN=ca'")
            .await
        {
            QueryResponse::Err { code: 1064, .. } => {}
            other => panic!("REQUIRE ISSUER must be rejected, got {other:?}"),
        }
        let loaded = rusql_core::PrivilegeStore::load(&server.data_dir).unwrap();
        assert!(loaded.get_account("m184ssl", "%").unwrap().require_ssl);
        assert!(!loaded.get_account("m184plain", "%").unwrap().require_ssl);

        assert_eq!(
            plaintext_require_ssl_errno(server.addr, "m184ssl", "secret").await,
            1045
        );
        let mut plain = server.connect_as("m184plain", "secret").await;
        assert!(matches!(
            plain.query("SELECT 1").await,
            QueryResponse::Rows { .. }
        ));
        assert_eq!(
            plain
                .try_change_user("m184ssl", "secret")
                .await
                .unwrap_err(),
            1045
        );
        assert!(matches!(
            plain.query("SELECT 1").await,
            QueryResponse::Rows { .. }
        ));
        tls_require_ssl_select(server.addr, &der, "m184ssl", "secret").await;

        assert!(matches!(
            admin.query("ALTER USER 'm184ssl'@'%' REQUIRE NONE").await,
            QueryResponse::Ok { .. }
        ));
        let mut cleared = server.connect_as("m184ssl", "secret").await;
        assert!(matches!(
            cleared.query("SELECT 1").await,
            QueryResponse::Rows { .. }
        ));
        assert!(matches!(
            admin.query("ALTER USER 'm184ssl'@'%' REQUIRE SSL").await,
            QueryResponse::Ok { .. }
        ));
        assert_eq!(
            plaintext_require_ssl_errno(server.addr, "m184ssl", "secret").await,
            1045
        );

        admin.quit().await;
        plain.quit().await;
        cleared.quit().await;
        let _ = key_path;
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }
}

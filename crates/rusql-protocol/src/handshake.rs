//! MySQL connection-phase handshake (protocol version 10).

use crate::framing::{read_packet_seq, write_packet};
use crate::ProtocolError;
use tokio::io::{AsyncRead, AsyncWrite};

pub use crate::command::SERVER_CAPABILITIES;

const CLIENT_PROTOCOL_41: u32 = 0x0000_0200;
const CLIENT_PLUGIN_AUTH: u32 = 0x0008_0000;
const CLIENT_SECURE_CONNECTION: u32 = 0x0000_8000;
const CLIENT_PLUGIN_AUTH_LENENC: u32 = 0x0020_0000;
const CLIENT_SSL: u32 = 0x0000_0800;
const CLIENT_CONNECT_ATTRS: u32 = 0x0010_0000;

use crate::auth::{
    auth_more_data_fast_auth_ok, auth_more_data_full_auth_required, auth_more_data_public_key,
    is_empty_password_auth, is_public_key_request, verify_auth_with_fallback,
    verify_caching_sha2_fast, CachingSha2RsaKeys,
};

const SERVER_STATUS_AUTOCOMMIT: u16 = 0x0002;
const AUTH_PLUGIN_NATIVE: &str = crate::auth::AUTH_PLUGIN_NATIVE;
const AUTH_PLUGIN_CACHING_SHA2: &str = crate::auth::AUTH_PLUGIN_CACHING_SHA2;

const SUPPORTED_AUTH_PLUGINS: &[&str] = &[AUTH_PLUGIN_NATIVE, AUTH_PLUGIN_CACHING_SHA2];

/// Server-side handshake configuration.
#[derive(Debug, Clone)]
pub struct HandshakeConfig {
    pub server_version: String,
    pub auth_plugin: String,
    /// When `Some`, verify password for --auth-user using caching_sha2 or native plugin.
    pub auth_credentials: Option<AuthCredentials>,
    /// RSA keys for caching_sha2 full-auth (generated when password auth is enabled).
    pub caching_sha2_rsa: Option<crate::auth::CachingSha2RsaKeys>,
}

impl HandshakeConfig {
    /// Ensure RSA keys exist when password verification is enabled with caching_sha2.
    pub fn ensure_caching_sha2_rsa(&mut self) {
        if self.auth_credentials.is_some()
            && self.auth_plugin == AUTH_PLUGIN_CACHING_SHA2
            && self.caching_sha2_rsa.is_none()
        {
            self.caching_sha2_rsa = CachingSha2RsaKeys::generate().ok();
        }
    }
}

/// Credentials for native password verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthCredentials {
    pub username: String,
    pub password: String,
}

impl Default for HandshakeConfig {
    fn default() -> Self {
        Self {
            server_version: "8.0.33-rusql".to_string(),
            auth_plugin: AUTH_PLUGIN_CACHING_SHA2.to_string(),
            auth_credentials: None,
            caching_sha2_rsa: None,
        }
    }
}

/// Established session metadata after a successful handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeSession {
    pub connection_id: u32,
    pub username: String,
    pub database: Option<String>,
    pub client_capabilities: u32,
}

/// Initial Handshake v10 packet payload (server → client).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialHandshake {
    pub connection_id: u32,
    pub scramble: [u8; 20],
    pub server_version: String,
    pub auth_plugin_name: String,
}

impl InitialHandshake {
    /// Encode the handshake payload (without packet framing).
    pub fn encode_payload(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.push(10);
        payload.extend_from_slice(self.server_version.as_bytes());
        payload.push(0);
        payload.extend_from_slice(&self.connection_id.to_le_bytes());
        payload.extend_from_slice(&self.scramble[..8]);
        payload.push(0);

        let caps = SERVER_CAPABILITIES;
        payload.extend_from_slice(&(caps as u16).to_le_bytes());
        payload.push(255);
        payload.extend_from_slice(&SERVER_STATUS_AUTOCOMMIT.to_le_bytes());
        payload.extend_from_slice(&((caps >> 16) as u16).to_le_bytes());
        payload.push(21);
        payload.extend_from_slice(&[0u8; 10]);

        let mut part2 = Vec::with_capacity(13);
        part2.extend_from_slice(&self.scramble[8..20]);
        part2.push(0);
        while part2.len() < 13 {
            part2.push(0);
        }
        payload.extend_from_slice(&part2[..13]);
        payload.extend_from_slice(self.auth_plugin_name.as_bytes());
        payload.push(0);
        payload
    }

    /// Decode an Initial Handshake payload.
    pub fn decode_payload(payload: &[u8]) -> Result<Self, ProtocolError> {
        if payload.is_empty() || payload[0] != 10 {
            return Err(ProtocolError::handshake_failed());
        }
        let mut pos = 1usize;
        let server_version = read_null_string(payload, &mut pos)?;
        if payload.len() < pos + 4 + 8 + 1 + 2 + 1 + 2 + 2 + 1 + 10 {
            return Err(ProtocolError::invalid_packet());
        }
        let connection_id = u32::from_le_bytes(payload[pos..pos + 4].try_into().unwrap());
        pos += 4;
        let mut scramble = [0u8; 20];
        scramble[..8].copy_from_slice(&payload[pos..pos + 8]);
        pos += 8 + 1 + 2 + 1 + 2 + 2 + 1 + 10;
        let auth_len = payload[pos - 11];
        let part2_len = auth_len.saturating_sub(8).max(13) as usize;
        if payload.len() < pos + part2_len {
            return Err(ProtocolError::invalid_packet());
        }
        let copy_len = (part2_len).min(12);
        scramble[8..8 + copy_len].copy_from_slice(&payload[pos..pos + copy_len]);
        pos += part2_len;
        let auth_plugin_name = read_null_string(payload, &mut pos)?;
        Ok(Self {
            connection_id,
            scramble,
            server_version,
            auth_plugin_name,
        })
    }
}

/// Client Handshake Response payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeResponse {
    pub capabilities: u32,
    pub username: String,
    pub auth_response: Vec<u8>,
    pub database: Option<String>,
    pub auth_plugin: Option<String>,
    pub connect_attributes: Vec<(String, String)>,
}

impl HandshakeResponse {
    /// Encode a minimal handshake response (for tests).
    pub fn encode_payload(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&self.capabilities.to_le_bytes());
        payload.extend_from_slice(&16_777_215u32.to_le_bytes());
        payload.push(255);
        payload.extend_from_slice(&[0u8; 23]);
        payload.extend_from_slice(self.username.as_bytes());
        payload.push(0);

        if self.capabilities & CLIENT_PLUGIN_AUTH_LENENC != 0 {
            write_lenenc_int(&mut payload, self.auth_response.len() as u64);
            payload.extend_from_slice(&self.auth_response);
        } else if self.capabilities & CLIENT_SECURE_CONNECTION != 0 {
            payload.push(self.auth_response.len() as u8);
            payload.extend_from_slice(&self.auth_response);
        } else {
            payload.extend_from_slice(&self.auth_response);
            payload.push(0);
        }

        if self.capabilities & 0x0000_0008 != 0 {
            if let Some(ref db) = self.database {
                payload.extend_from_slice(db.as_bytes());
                payload.push(0);
            }
        }

        if self.capabilities & CLIENT_PLUGIN_AUTH != 0 {
            if let Some(ref plugin) = self.auth_plugin {
                payload.extend_from_slice(plugin.as_bytes());
                payload.push(0);
            }
        }

        if self.capabilities & CLIENT_CONNECT_ATTRS != 0 && !self.connect_attributes.is_empty() {
            let mut attrs = Vec::new();
            for (key, value) in &self.connect_attributes {
                write_lenenc_string(&mut attrs, key);
                write_lenenc_string(&mut attrs, value);
            }
            write_lenenc_int(&mut payload, attrs.len() as u64);
            payload.extend_from_slice(&attrs);
        }
        payload
    }

    /// Decode client handshake response payload.
    pub fn decode_payload(payload: &[u8]) -> Result<Self, ProtocolError> {
        if payload.len() < 4 + 4 + 1 + 23 {
            return Err(ProtocolError::invalid_packet());
        }
        let capabilities = u32::from_le_bytes(payload[0..4].try_into().unwrap());
        let mut pos = 4 + 4 + 1 + 23;
        let username = read_null_string(payload, &mut pos)?;

        let auth_response = if capabilities & CLIENT_PLUGIN_AUTH_LENENC != 0 {
            read_lenenc_bytes(payload, &mut pos)?
        } else if capabilities & CLIENT_SECURE_CONNECTION != 0 {
            if pos >= payload.len() {
                return Err(ProtocolError::invalid_packet());
            }
            let len = payload[pos] as usize;
            pos += 1;
            if payload.len() < pos + len {
                return Err(ProtocolError::invalid_packet());
            }
            let bytes = payload[pos..pos + len].to_vec();
            pos += len;
            bytes
        } else {
            read_null_terminated_bytes(payload, &mut pos)?
        };

        let database = if capabilities & 0x0000_0008 != 0 && pos < payload.len() {
            let db = read_null_string(payload, &mut pos).ok();
            db.filter(|s| !s.is_empty())
        } else {
            None
        };

        let auth_plugin = if capabilities & CLIENT_PLUGIN_AUTH != 0 && pos < payload.len() {
            read_null_string(payload, &mut pos).ok()
        } else {
            None
        };

        let connect_attributes = if capabilities & CLIENT_CONNECT_ATTRS != 0 && pos < payload.len()
        {
            skip_connect_attributes(payload, &mut pos).unwrap_or_default()
        } else {
            vec![]
        };

        Ok(Self {
            capabilities,
            username,
            auth_response,
            database,
            auth_plugin,
            connect_attributes,
        })
    }
}

/// OK packet payload (without framing).
pub fn encode_ok_payload() -> Vec<u8> {
    encode_ok_for_client(0)
}

/// OK packet with optional session-state trailer when negotiated (WL#6257).
pub fn encode_ok_for_client(client_caps: u32) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.push(0x00);
    write_lenenc_int(&mut payload, 0);
    write_lenenc_int(&mut payload, 0);
    payload.extend_from_slice(&SERVER_STATUS_AUTOCOMMIT.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes());
    let _ = client_caps;
    payload
}

/// ERR packet payload.
pub fn encode_err_payload(code: u16, message: &str) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.push(0xFF);
    payload.extend_from_slice(&code.to_le_bytes());
    payload.push(b'#');
    payload.extend_from_slice(b"HY000");
    payload.extend_from_slice(message.as_bytes());
    payload
}

fn make_scramble(connection_id: u32) -> [u8; 20] {
    let seed = connection_id.to_le_bytes();
    let mut s = [0u8; 20];
    for (i, b) in s.iter_mut().enumerate() {
        *b = seed[i % 4] ^ (i as u8).wrapping_mul(31) ^ 0x5A;
    }
    s
}

/// Perform server-side handshake on a TCP stream.
pub async fn server_handshake<S>(
    stream: &mut S,
    config: &HandshakeConfig,
    connection_id: u32,
) -> Result<HandshakeSession, ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let handshake = InitialHandshake {
        connection_id,
        scramble: make_scramble(connection_id),
        server_version: config.server_version.clone(),
        auth_plugin_name: config.auth_plugin.clone(),
    };

    write_packet(stream, 0, &handshake.encode_payload()).await?;

    let response_payload = read_packet_seq(stream, 1).await?;
    let response = HandshakeResponse::decode_payload(&response_payload)?;

    if response.capabilities & CLIENT_PROTOCOL_41 == 0 {
        return Err(ProtocolError::handshake_failed());
    }

    if let Some(ref plugin) = response.auth_plugin {
        if plugin != &config.auth_plugin && !SUPPORTED_AUTH_PLUGINS.contains(&plugin.as_str()) {
            let err = encode_err_payload(1251, &rusql_i18n::messages::protocol_unsupported_auth());
            let _ = write_packet(stream, 2, &err).await;
            return Err(ProtocolError::Message(
                rusql_i18n::messages::protocol_unsupported_auth(),
            ));
        }
    }

    if let Some(ref creds) = config.auth_credentials {
        if response.username != creds.username {
            return deny_access(stream, 2).await;
        }

        let plugin = response
            .auth_plugin
            .as_deref()
            .unwrap_or(config.auth_plugin.as_str());

        if plugin == AUTH_PLUGIN_CACHING_SHA2 {
            return finish_caching_sha2_handshake(stream, config, &handshake, &response, creds)
                .await;
        }

        if !verify_auth_with_fallback(
            &creds.password,
            &handshake.scramble,
            &response.auth_response,
            response.auth_plugin.as_deref(),
        ) {
            return deny_access(stream, 2).await;
        }

        write_packet(stream, 2, &encode_ok_for_client(response.capabilities)).await?;
    } else if config.auth_plugin == AUTH_PLUGIN_CACHING_SHA2 {
        // Match MySQL 8.0: empty/null-byte auth → direct OK; real scramble → AuthMoreData then OK.
        if is_empty_password_auth(&response.auth_response) {
            write_packet(stream, 2, &encode_ok_for_client(response.capabilities)).await?;
        } else {
            write_packet(stream, 2, &auth_more_data_fast_auth_ok()).await?;
            write_packet(stream, 3, &encode_ok_for_client(response.capabilities)).await?;
        }
    } else {
        write_packet(stream, 2, &encode_ok_for_client(response.capabilities)).await?;
    }

    Ok(HandshakeSession {
        connection_id,
        username: response.username,
        database: response.database,
        client_capabilities: response.capabilities,
    })
}

async fn deny_access<S>(stream: &mut S, seq: u8) -> Result<HandshakeSession, ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let err = encode_err_payload(1045, &rusql_i18n::messages::protocol_access_denied());
    let _ = write_packet(stream, seq, &err).await;
    Err(ProtocolError::Message(
        rusql_i18n::messages::protocol_access_denied(),
    ))
}

async fn finish_caching_sha2_handshake<S>(
    stream: &mut S,
    config: &HandshakeConfig,
    handshake: &InitialHandshake,
    response: &HandshakeResponse,
    creds: &AuthCredentials,
) -> Result<HandshakeSession, ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let scramble = &handshake.scramble;
    let password = &creds.password;

    if password.is_empty() && is_empty_password_auth(&response.auth_response) {
        write_packet(stream, 2, &encode_ok_for_client(response.capabilities)).await?;
        return Ok(HandshakeSession {
            connection_id: handshake.connection_id,
            username: response.username.clone(),
            database: response.database.clone(),
            client_capabilities: response.capabilities,
        });
    }

    if verify_caching_sha2_fast(password, scramble, &response.auth_response) {
        write_packet(stream, 2, &auth_more_data_fast_auth_ok()).await?;
        write_packet(stream, 3, &encode_ok_for_client(response.capabilities)).await?;
        return Ok(HandshakeSession {
            connection_id: handshake.connection_id,
            username: response.username.clone(),
            database: response.database.clone(),
            client_capabilities: response.capabilities,
        });
    }

    if response.auth_response.len() == 32 {
        return deny_access(stream, 2).await;
    }

    let rsa_keys = config
        .caching_sha2_rsa
        .as_ref()
        .ok_or_else(|| ProtocolError::Message("caching_sha2 RSA keys not configured".into()))?;

    write_packet(stream, 2, &auth_more_data_full_auth_required()).await?;
    let mut seq = 3u8;
    let client_step = read_packet_seq(stream, seq).await?;
    seq = seq.wrapping_add(1);

    let authenticated = if is_public_key_request(&client_step) {
        let pem = rsa_keys.public_key_pem();
        write_packet(stream, seq, &auth_more_data_public_key(&pem)).await?;
        seq = seq.wrapping_add(1);
        let encrypted = read_packet_seq(stream, seq).await?;
        seq = seq.wrapping_add(1);
        rsa_keys
            .decrypt_password(&encrypted, scramble)
            .map(|p| p == *password)
            .unwrap_or(false)
    } else if response.capabilities & CLIENT_SSL != 0 {
        plaintext_password_from_payload(&client_step) == *password
    } else {
        false
    };

    if !authenticated {
        return deny_access(stream, seq).await;
    }

    write_packet(stream, seq, &encode_ok_for_client(response.capabilities)).await?;
    Ok(HandshakeSession {
        connection_id: handshake.connection_id,
        username: response.username.clone(),
        database: response.database.clone(),
        client_capabilities: response.capabilities,
    })
}

fn plaintext_password_from_payload(payload: &[u8]) -> String {
    let end = payload
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(payload.len());
    String::from_utf8_lossy(&payload[..end]).into_owned()
}

fn skip_connect_attributes(
    buf: &[u8],
    pos: &mut usize,
) -> Result<Vec<(String, String)>, ProtocolError> {
    let blob_len = read_lenenc_int(buf, pos)? as usize;
    if buf.len() < *pos + blob_len {
        return Err(ProtocolError::invalid_packet());
    }
    let end = *pos + blob_len;
    let mut attrs = Vec::new();
    while *pos < end {
        let key = read_lenenc_string(buf, pos)?;
        let value = read_lenenc_string(buf, pos)?;
        attrs.push((key, value));
    }
    *pos = end;
    Ok(attrs)
}

fn read_lenenc_string(buf: &[u8], pos: &mut usize) -> Result<String, ProtocolError> {
    let len = read_lenenc_int(buf, pos)? as usize;
    if buf.len() < *pos + len {
        return Err(ProtocolError::invalid_packet());
    }
    let s = std::str::from_utf8(&buf[*pos..*pos + len])
        .map_err(|_| ProtocolError::invalid_packet())?
        .to_string();
    *pos += len;
    Ok(s)
}

fn read_null_string(buf: &[u8], pos: &mut usize) -> Result<String, ProtocolError> {
    let start = *pos;
    while *pos < buf.len() && buf[*pos] != 0 {
        *pos += 1;
    }
    if *pos >= buf.len() {
        return Err(ProtocolError::invalid_packet());
    }
    let s = std::str::from_utf8(&buf[start..*pos])
        .map_err(|_| ProtocolError::invalid_packet())?
        .to_string();
    *pos += 1;
    Ok(s)
}

fn read_null_terminated_bytes(buf: &[u8], pos: &mut usize) -> Result<Vec<u8>, ProtocolError> {
    let start = *pos;
    while *pos < buf.len() && buf[*pos] != 0 {
        *pos += 1;
    }
    if *pos >= buf.len() {
        return Err(ProtocolError::invalid_packet());
    }
    let bytes = buf[start..*pos].to_vec();
    *pos += 1;
    Ok(bytes)
}

fn read_lenenc_bytes(buf: &[u8], pos: &mut usize) -> Result<Vec<u8>, ProtocolError> {
    let len = read_lenenc_int(buf, pos)? as usize;
    if buf.len() < *pos + len {
        return Err(ProtocolError::invalid_packet());
    }
    let bytes = buf[*pos..*pos + len].to_vec();
    *pos += len;
    Ok(bytes)
}

fn read_lenenc_int(buf: &[u8], pos: &mut usize) -> Result<u64, ProtocolError> {
    if *pos >= buf.len() {
        return Err(ProtocolError::invalid_packet());
    }
    let first = buf[*pos];
    *pos += 1;
    match first {
        n @ 0..=250 => Ok(u64::from(n)),
        0xFC => {
            if buf.len() < *pos + 2 {
                return Err(ProtocolError::invalid_packet());
            }
            let v = u16::from_le_bytes(buf[*pos..*pos + 2].try_into().unwrap());
            *pos += 2;
            Ok(u64::from(v))
        }
        0xFD => {
            if buf.len() < *pos + 3 {
                return Err(ProtocolError::invalid_packet());
            }
            let v = u32::from_le_bytes([buf[*pos], buf[*pos + 1], buf[*pos + 2], 0]);
            *pos += 3;
            Ok(u64::from(v))
        }
        0xFE => {
            if buf.len() < *pos + 8 {
                return Err(ProtocolError::invalid_packet());
            }
            let v = u64::from_le_bytes(buf[*pos..*pos + 8].try_into().unwrap());
            *pos += 8;
            Ok(v)
        }
        _ => Err(ProtocolError::invalid_packet()),
    }
}

fn write_lenenc_int(buf: &mut Vec<u8>, n: u64) {
    if n < 251 {
        buf.push(n as u8);
    } else if n < 65_536 {
        buf.push(0xFC);
        buf.extend_from_slice(&(n as u16).to_le_bytes());
    } else if n < 16_777_216 {
        buf.push(0xFD);
        buf.extend_from_slice(&(n as u32).to_le_bytes()[..3]);
    } else {
        buf.push(0xFE);
        buf.extend_from_slice(&n.to_le_bytes());
    }
}

fn write_lenenc_string(buf: &mut Vec<u8>, s: &str) {
    write_lenenc_int(buf, s.len() as u64);
    buf.extend_from_slice(s.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::PacketWriter;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    #[test]
    fn initial_handshake_roundtrip_caching_sha2() {
        let hs = InitialHandshake {
            connection_id: 42,
            scramble: make_scramble(42),
            server_version: "8.0.33-rusql".into(),
            auth_plugin_name: AUTH_PLUGIN_CACHING_SHA2.into(),
        };
        let encoded = hs.encode_payload();
        let decoded = InitialHandshake::decode_payload(&encoded).unwrap();
        assert_eq!(decoded.connection_id, 42);
        assert_eq!(decoded.auth_plugin_name, AUTH_PLUGIN_CACHING_SHA2);
    }

    #[test]
    fn initial_handshake_roundtrip_native() {
        let hs = InitialHandshake {
            connection_id: 42,
            scramble: make_scramble(42),
            server_version: "8.0.33-rusql".into(),
            auth_plugin_name: AUTH_PLUGIN_NATIVE.into(),
        };
        let encoded = hs.encode_payload();
        let decoded = InitialHandshake::decode_payload(&encoded).unwrap();
        assert_eq!(decoded.auth_plugin_name, AUTH_PLUGIN_NATIVE);
    }

    #[test]
    fn handshake_response_roundtrip() {
        let resp = HandshakeResponse {
            capabilities: SERVER_CAPABILITIES,
            username: "root".into(),
            auth_response: vec![0],
            database: None,
            auth_plugin: Some(AUTH_PLUGIN_NATIVE.into()),
            connect_attributes: vec![],
        };
        let encoded = resp.encode_payload();
        let decoded = HandshakeResponse::decode_payload(&encoded).unwrap();
        assert_eq!(decoded.username, "root");
    }

    #[test]
    fn ok_packet_starts_with_zero() {
        let ok = encode_ok_payload();
        assert_eq!(ok[0], 0x00);
    }

    #[test]
    fn handshake_ok_has_no_empty_session_track_trailer() {
        use crate::command::{CLIENT_SESSION_TRACK, SERVER_CAPABILITIES};
        let caps = CLIENT_SESSION_TRACK | SERVER_CAPABILITIES;
        let ok = encode_ok_for_client(caps);
        assert_eq!(ok.len(), 7, "OK packet bytes: {:02x?}", ok);
    }

    #[tokio::test]
    async fn server_handshake_integration() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            server_handshake(&mut stream, &HandshakeConfig::default(), 1)
                .await
                .unwrap()
        });

        let mut client = TcpStream::connect(addr).await.unwrap();

        let mut hdr = [0u8; 4];
        client.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        let mut payload = vec![0u8; len];
        client.read_exact(&mut payload).await.unwrap();
        let hs = InitialHandshake::decode_payload(&payload).unwrap();
        assert_eq!(hs.auth_plugin_name, AUTH_PLUGIN_CACHING_SHA2);

        let response = HandshakeResponse {
            capabilities: CLIENT_PROTOCOL_41
                | CLIENT_PLUGIN_AUTH
                | CLIENT_SECURE_CONNECTION
                | CLIENT_PLUGIN_AUTH_LENENC,
            username: "root".into(),
            auth_response: vec![],
            database: None,
            auth_plugin: Some(AUTH_PLUGIN_CACHING_SHA2.into()),
            connect_attributes: vec![],
        };
        let resp_payload = response.encode_payload();
        let framed = PacketWriter::encode(1, &resp_payload);
        client.write_all(&framed).await.unwrap();

        client.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        payload.resize(len, 0);
        client.read_exact(&mut payload).await.unwrap();
        assert_eq!(payload[0], 0x00, "empty auth expects direct OK");
        assert_eq!(payload.len(), 7, "OK packet bytes: {:02x?}", payload);

        let session = server.await.unwrap();
        assert_eq!(session.username, "root");
        assert_eq!(session.connection_id, 1);
    }

    #[tokio::test]
    async fn caching_sha2_null_byte_auth_gets_direct_ok() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            server_handshake(&mut stream, &HandshakeConfig::default(), 1)
                .await
                .unwrap()
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        let mut hdr = [0u8; 4];
        client.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        let mut payload = vec![0u8; len];
        client.read_exact(&mut payload).await.unwrap();

        // libmysql sends a single 0x00 byte for empty password (not an empty auth slice).
        let response = HandshakeResponse {
            capabilities: CLIENT_PROTOCOL_41
                | CLIENT_PLUGIN_AUTH
                | CLIENT_SECURE_CONNECTION
                | CLIENT_PLUGIN_AUTH_LENENC,
            username: "root".into(),
            auth_response: vec![0x00],
            database: None,
            auth_plugin: Some(AUTH_PLUGIN_CACHING_SHA2.into()),
            connect_attributes: vec![],
        };
        client
            .write_all(&PacketWriter::encode(1, &response.encode_payload()))
            .await
            .unwrap();

        client.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        payload.resize(len, 0);
        client.read_exact(&mut payload).await.unwrap();
        assert_eq!(payload[0], 0x00, "null-byte auth expects direct OK");
        assert_eq!(payload.len(), 7, "OK packet bytes: {:02x?}", payload);

        server.await.unwrap();
    }

    #[tokio::test]
    async fn caching_sha2_nonempty_auth_gets_fast_auth_then_ok() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            server_handshake(&mut stream, &HandshakeConfig::default(), 1)
                .await
                .unwrap()
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        let mut hdr = [0u8; 4];
        client.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        let mut payload = vec![0u8; len];
        client.read_exact(&mut payload).await.unwrap();

        let response = HandshakeResponse {
            capabilities: CLIENT_PROTOCOL_41
                | CLIENT_PLUGIN_AUTH
                | CLIENT_SECURE_CONNECTION
                | CLIENT_PLUGIN_AUTH_LENENC,
            username: "root".into(),
            auth_response: vec![0xAB; 32],
            database: None,
            auth_plugin: Some(AUTH_PLUGIN_CACHING_SHA2.into()),
            connect_attributes: vec![],
        };
        client
            .write_all(&PacketWriter::encode(1, &response.encode_payload()))
            .await
            .unwrap();

        client.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        payload.resize(len, 0);
        client.read_exact(&mut payload).await.unwrap();
        assert_eq!(payload, [0x01, 0x03]);

        client.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        payload.resize(len, 0);
        client.read_exact(&mut payload).await.unwrap();
        assert_eq!(payload[0], 0x00);

        let _ = server.await;
    }
}

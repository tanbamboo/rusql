//! MySQL command-phase client commands.

use crate::ProtocolError;

pub const COM_QUIT: u8 = 0x01;
pub const COM_QUERY: u8 = 0x03;
pub const COM_STMT_PREPARE: u8 = 0x16;
pub const COM_STMT_EXECUTE: u8 = 0x17;
pub const COM_STMT_CLOSE: u8 = 0x19;

/// WL#12542 — query attributes on COM_QUERY when negotiated.
pub const CLIENT_QUERY_ATTRIBUTES: u32 = 0x0800_0000;

/// WL#7766 — OK packet instead of EOF at end of resultsets.
pub const CLIENT_DEPRECATE_EOF: u32 = 0x0100_0000;

/// WL#6257 — session state changes in OK packets.
pub const CLIENT_SESSION_TRACK: u32 = 0x0080_0000;

/// Server capability flags advertised during handshake (includes query attributes).
/// SSL is omitted — rusql does not implement TLS upgrade on the wire.
pub const SERVER_CAPABILITIES: u32 = (0x000F_F7DF & !0x0000_0800)
    | CLIENT_QUERY_ATTRIBUTES
    | CLIENT_DEPRECATE_EOF;

const MYSQL_TYPE_LONGLONG: u8 = 0x08;
const MYSQL_TYPE_VAR_STRING: u8 = 0x0F;
const MYSQL_TYPE_STRING: u8 = 0xFE;

/// Max plausible WL#12542 query-attribute count; larger values are treated as raw SQL.
const MAX_QUERY_ATTR_PARAM_COUNT: usize = 32;

/// Parsed client command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientCommand {
    Quit,
    Query(String),
    StmtPrepare(String),
    StmtExecute { stmt_id: u32, payload: Vec<u8> },
    StmtClose { stmt_id: u32 },
    Unknown(u8),
}

/// True when client and server both advertise query attributes.
pub fn query_attributes_negotiated(client_caps: u32, server_caps: u32) -> bool {
    client_caps & server_caps & CLIENT_QUERY_ATTRIBUTES != 0
}

/// True when resultsets should end with OK instead of legacy EOF (WL#7766).
pub fn deprecate_eof_negotiated(client_caps: u32, server_caps: u32) -> bool {
    client_caps & server_caps & CLIENT_DEPRECATE_EOF != 0
}

/// True when OK packets include session-state-change info (WL#6257).
pub fn session_track_negotiated(client_caps: u32, server_caps: u32) -> bool {
    client_caps & server_caps & CLIENT_SESSION_TRACK != 0
}

/// Build COM_QUERY with MySQL 8.0 query-attributes preamble (param_count=0, set_count=1).
pub fn encode_com_query_with_attributes(sql: &str) -> Vec<u8> {
    let mut p = vec![COM_QUERY, 0x00, 0x01];
    p.extend_from_slice(sql.as_bytes());
    p
}

/// Parse COM_* command payload (first byte is command code).
pub fn parse_command(payload: &[u8], client_caps: u32) -> Result<ClientCommand, ProtocolError> {
    parse_command_with_server_caps(payload, client_caps, SERVER_CAPABILITIES)
}

/// Parse COM_* with explicit server capability mask (for tests).
pub fn parse_command_with_server_caps(
    payload: &[u8],
    client_caps: u32,
    server_caps: u32,
) -> Result<ClientCommand, ProtocolError> {
    if payload.is_empty() {
        return Err(ProtocolError::invalid_packet());
    }
    match payload[0] {
        COM_QUIT => Ok(ClientCommand::Quit),
        COM_QUERY => {
            let body = &payload[1..];
            let sql_start = com_query_sql_start(body, client_caps, server_caps)?;
            let sql = std::str::from_utf8(&body[sql_start..])
                .map_err(|_| ProtocolError::invalid_packet())?
                .trim_end_matches('\0')
                .to_string();
            Ok(ClientCommand::Query(sql))
        }
        COM_STMT_PREPARE => {
            let sql = std::str::from_utf8(&payload[1..])
                .map_err(|_| ProtocolError::invalid_packet())?
                .trim_end_matches('\0')
                .to_string();
            Ok(ClientCommand::StmtPrepare(sql))
        }
        COM_STMT_EXECUTE => {
            if payload.len() < 5 {
                return Err(ProtocolError::invalid_packet());
            }
            let stmt_id = u32::from_le_bytes(payload[1..5].try_into().unwrap());
            Ok(ClientCommand::StmtExecute {
                stmt_id,
                payload: payload.to_vec(),
            })
        }
        COM_STMT_CLOSE => {
            if payload.len() < 5 {
                return Err(ProtocolError::invalid_packet());
            }
            let stmt_id = u32::from_le_bytes(payload[1..5].try_into().unwrap());
            Ok(ClientCommand::StmtClose { stmt_id })
        }
        other => Ok(ClientCommand::Unknown(other)),
    }
}

/// Byte offset where SQL starts in COM_QUERY body (after optional attribute preamble).
fn com_query_sql_start(
    body: &[u8],
    client_caps: u32,
    server_caps: u32,
) -> Result<usize, ProtocolError> {
    if !query_attributes_negotiated(client_caps, server_caps) {
        return Ok(0);
    }
    if body.is_empty() {
        return Ok(0);
    }
    let mut peek = 0usize;
    let param_count = match read_lenenc_int(body, &mut peek) {
        Ok(n) => n as usize,
        Err(_) => return Ok(0),
    };
    if param_count > MAX_QUERY_ATTR_PARAM_COUNT {
        return Ok(0);
    }
    match skip_com_query_attributes(body) {
        Ok(start) => Ok(start),
        Err(_) => Ok(0),
    }
}

/// Skip WL#12542 query-attributes block; return byte offset where SQL starts.
fn skip_com_query_attributes(body: &[u8]) -> Result<usize, ProtocolError> {
    let mut pos = 0usize;
    let param_count = read_lenenc_int(body, &mut pos)? as usize;
    let set_count_pos = pos;
    let param_set_count = read_lenenc_int(body, &mut pos)?;

    if param_count == 0 {
        // Official client may omit parameter_set_count when zero params; if the
        // next lenenc byte looks like SQL (e.g. 'I' = 73), only skip param_count.
        if param_set_count > 1 {
            return Ok(set_count_pos);
        }
        return Ok(pos);
    }

    let bitmap_len = param_count.div_ceil(8);
    if body.len() < pos + bitmap_len + 1 {
        return Err(ProtocolError::invalid_packet());
    }
    let null_bitmap = &body[pos..pos + bitmap_len];
    pos += bitmap_len;
    let new_params = body[pos];
    if new_params != 1 {
        return Err(ProtocolError::invalid_packet());
    }
    pos += 1;

    let mut types = Vec::with_capacity(param_count);
    for _ in 0..param_count {
        if body.len() < pos + 2 {
            return Err(ProtocolError::invalid_packet());
        }
        types.push(u16::from_le_bytes([body[pos], body[pos + 1]]));
        pos += 2;
        let name_len = read_lenenc_int(body, &mut pos)? as usize;
        if body.len() < pos + name_len {
            return Err(ProtocolError::invalid_packet());
        }
        pos += name_len;
    }

    for (i, ty) in types.iter().enumerate() {
        if is_null(null_bitmap, i) {
            continue;
        }
        pos = skip_binary_value(body, pos, *ty as u8)?;
    }
    Ok(pos)
}

fn is_null(bitmap: &[u8], index: usize) -> bool {
    let byte = bitmap[index / 8];
    (byte & (1 << (index % 8))) != 0
}

fn skip_binary_value(body: &[u8], pos: usize, col_type: u8) -> Result<usize, ProtocolError> {
    match col_type {
        MYSQL_TYPE_LONGLONG => {
            if body.len() < pos + 8 {
                return Err(ProtocolError::invalid_packet());
            }
            Ok(pos + 8)
        }
        MYSQL_TYPE_VAR_STRING | MYSQL_TYPE_STRING => {
            let (len, hlen) = read_lenenc_int_at(body, pos)?;
            let end = pos + hlen + len as usize;
            if body.len() < end {
                return Err(ProtocolError::invalid_packet());
            }
            Ok(end)
        }
        _ => Err(ProtocolError::Message(format!(
            "unsupported query attribute type: 0x{col_type:02X}"
        ))),
    }
}

fn read_lenenc_int_at(buf: &[u8], pos: usize) -> Result<(u64, usize), ProtocolError> {
    if pos >= buf.len() {
        return Err(ProtocolError::invalid_packet());
    }
    let mut p = pos;
    let n = read_lenenc_int(buf, &mut p)?;
    Ok((n, p - pos))
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

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY_CAPS: u32 = 0x0000_0200 | 0x0008_0000 | 0x0000_8000 | 0x0020_0000;
    const MYSQL_CLI_CAPS: u32 = LEGACY_CAPS | CLIENT_QUERY_ATTRIBUTES;

    #[test]
    fn parse_com_query_legacy() {
        let mut p = vec![COM_QUERY];
        p.extend_from_slice(b"SELECT 1");
        let cmd = parse_command(&p, LEGACY_CAPS).unwrap();
        assert_eq!(cmd, ClientCommand::Query("SELECT 1".into()));
    }

    #[test]
    fn parse_com_query_with_empty_attributes() {
        let p = encode_com_query_with_attributes("UPDATE t SET id = 2 WHERE id = 1");
        let cmd = parse_command(&p, MYSQL_CLI_CAPS).unwrap();
        assert_eq!(
            cmd,
            ClientCommand::Query("UPDATE t SET id = 2 WHERE id = 1".into())
        );
    }

    #[test]
    fn parse_com_query_attributes_not_negotiated_leaves_sql_unchanged() {
        let p = encode_com_query_with_attributes("SELECT 1");
        let cmd = parse_command(&p, LEGACY_CAPS).unwrap();
        assert_eq!(cmd, ClientCommand::Query("\0\x01SELECT 1".into()));
    }

    #[test]
    fn parse_com_query_attributes_only_param_count_byte() {
        let mut p = vec![COM_QUERY, 0x00];
        p.extend_from_slice(b"INSERT INTO t VALUES (1)");
        let cmd = parse_command(&p, MYSQL_CLI_CAPS).unwrap();
        assert_eq!(cmd, ClientCommand::Query("INSERT INTO t VALUES (1)".into()));
    }

    #[test]
    fn parse_com_query_plain_sql_when_attrs_negotiated() {
        let mut p = vec![COM_QUERY];
        p.extend_from_slice(b"SELECT 1");
        let caps = MYSQL_CLI_CAPS | CLIENT_DEPRECATE_EOF | CLIENT_SESSION_TRACK;
        let cmd = parse_command(&p, caps).unwrap();
        assert_eq!(cmd, ClientCommand::Query("SELECT 1".into()));
    }

    #[test]
    fn parse_com_query_wl12542_example_with_one_attribute() {
        // MySQL doc example: one named attribute then SQL.
        let mut body = vec![0x01, 0x01, 0x00, 0x01, 0xFE, 0x00, 0x01, b'a', 0x01, b'1'];
        body.extend_from_slice(b"select @@version_comment limit 1");
        let mut p = vec![COM_QUERY];
        p.extend_from_slice(&body);
        let cmd = parse_command(&p, MYSQL_CLI_CAPS).unwrap();
        assert_eq!(
            cmd,
            ClientCommand::Query("select @@version_comment limit 1".into())
        );
    }
}

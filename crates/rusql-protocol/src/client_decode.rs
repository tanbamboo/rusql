use crate::binary::decode_binary_resultset_row;

/// Decoded server response to a COM_QUERY.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryResponse {
    Ok {
        affected_rows: u64,
    },
    Err {
        code: u16,
        message: String,
    },
    Rows {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

/// True for legacy EOF or OK-as-EOF trailer at end of a text/binary resultset.
pub fn is_resultset_terminator(packet: &[u8]) -> bool {
    is_resultset_terminator_for_client(packet, false)
}

/// When `deprecate_eof` is true, also accept OK-as-EOF trailers (WL#7766).
pub fn is_resultset_terminator_for_client(packet: &[u8], deprecate_eof: bool) -> bool {
    is_resultset_terminator_with_caps(packet, deprecate_eof, deprecate_eof)
}

/// Resultset terminator with explicit session-track negotiation.
pub fn is_resultset_terminator_with_caps(
    packet: &[u8],
    deprecate_eof: bool,
    session_track: bool,
) -> bool {
    if packet.is_empty() {
        return false;
    }
    if packet[0] == 0xFE && packet.len() < 8 {
        return true;
    }
    deprecate_eof && is_ok_as_eof_packet(packet, session_track)
}

fn is_ok_as_eof_packet(packet: &[u8], session_track: bool) -> bool {
    if packet.first() != Some(&0x00) {
        return false;
    }
    let mut pos = 1usize;
    if parse_lenenc_int_opt(packet, &mut pos).is_none() {
        return false;
    }
    if parse_lenenc_int_opt(packet, &mut pos).is_none() {
        return false;
    }
    // status_flags (2) + warnings (2)
    if packet.len() < pos + 4 {
        return false;
    }
    pos += 4;
    if session_track && pos < packet.len() && parse_lenenc_int_opt(packet, &mut pos).is_none() {
        return false;
    }
    pos == packet.len()
}

fn parse_lenenc_int_opt(payload: &[u8], pos: &mut usize) -> Option<u64> {
    if *pos >= payload.len() {
        return None;
    }
    let first = payload[*pos];
    *pos += 1;
    match first {
        n @ 0..=250 => Some(u64::from(n)),
        0xFC => {
            if payload.len() < *pos + 2 {
                return None;
            }
            let v = u16::from_le_bytes(payload[*pos..*pos + 2].try_into().ok()?);
            *pos += 2;
            Some(u64::from(v))
        }
        0xFD => {
            if payload.len() < *pos + 3 {
                return None;
            }
            let v = u32::from_le_bytes([payload[*pos], payload[*pos + 1], payload[*pos + 2], 0]);
            *pos += 3;
            Some(u64::from(v))
        }
        0xFE => {
            if payload.len() < *pos + 8 {
                return None;
            }
            let v = u64::from_le_bytes(payload[*pos..*pos + 8].try_into().ok()?);
            *pos += 8;
            Some(v)
        }
        _ => None,
    }
}

/// Parse the first payload of a query response.
pub fn classify_query_payload(payload: &[u8]) -> Result<QueryResponse, String> {
    if payload.is_empty() {
        return Err("empty response".into());
    }
    match payload[0] {
        0x00 => Ok(QueryResponse::Ok {
            affected_rows: read_lenenc_int(payload, &mut 1),
        }),
        0xFF => {
            if payload.len() < 3 {
                return Err("truncated ERR packet".into());
            }
            let code = u16::from_le_bytes([payload[1], payload[2]]);
            let message = String::from_utf8_lossy(&payload[3..]).to_string();
            Ok(QueryResponse::Err { code, message })
        }
        _ => {
            let col_count = read_lenenc_int(payload, &mut 0) as usize;
            Ok(QueryResponse::Rows {
                columns: Vec::with_capacity(col_count),
                rows: vec![],
            })
        }
    }
}

/// Extract column name from a ColumnDefinition41 packet payload.
pub fn column_name_from_definition(payload: &[u8]) -> Option<String> {
    let mut pos = 0;
    for i in 0..5 {
        let s = read_lenenc_string(payload, &mut pos)?;
        if i == 4 {
            return Some(s);
        }
    }
    None
}

/// Extract MySQL column type byte from a ColumnDefinition41 packet.
pub fn mysql_type_from_column_definition(payload: &[u8]) -> Option<u8> {
    let mut pos = 0;
    for _ in 0..6 {
        read_lenenc_string(payload, &mut pos)?;
    }
    if payload.get(pos)? != &0x0c {
        return None;
    }
    pos += 1 + 2 + 4;
    payload.get(pos).copied()
}

/// Decode column names and wire types from resultset column definition packets.
pub fn decode_column_definitions(defs: &[Vec<u8>]) -> Option<(Vec<String>, Vec<u8>)> {
    let mut columns = Vec::with_capacity(defs.len());
    let mut types = Vec::with_capacity(defs.len());
    for def in defs {
        columns.push(column_name_from_definition(def)?);
        types.push(mysql_type_from_column_definition(def)?);
    }
    Some((columns, types))
}

/// Decode a binary `COM_STMT_EXECUTE` result row.
pub fn decode_binary_row(col_types: &[u8], payload: &[u8]) -> Option<Vec<String>> {
    decode_binary_resultset_row(col_types, payload)
}

/// Decode a text result row packet.
pub fn decode_text_row(payload: &[u8]) -> Option<Vec<String>> {
    let mut pos = 0;
    let mut values = Vec::new();
    while pos < payload.len() {
        if payload[pos] == 0xFB {
            pos += 1;
            values.push(String::new());
            continue;
        }
        values.push(read_lenenc_string(payload, &mut pos)?);
    }
    Some(values)
}

pub fn read_lenenc_int(payload: &[u8], pos: &mut usize) -> u64 {
    if *pos >= payload.len() {
        return 0;
    }
    let first = payload[*pos];
    *pos += 1;
    match first {
        n @ 0..=250 => n as u64,
        0xFC => {
            let v = u16::from_le_bytes([payload[*pos], payload[*pos + 1]]) as u64;
            *pos += 2;
            v
        }
        0xFD => {
            let v =
                u32::from_le_bytes([payload[*pos], payload[*pos + 1], payload[*pos + 2], 0]) as u64;
            *pos += 3;
            v
        }
        0xFE => {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&payload[*pos..*pos + 8]);
            *pos += 8;
            u64::from_le_bytes(buf)
        }
        _ => 0,
    }
}

pub fn read_lenenc_string(payload: &[u8], pos: &mut usize) -> Option<String> {
    let len = read_lenenc_int(payload, pos) as usize;
    if *pos + len > payload.len() {
        return None;
    }
    let s = String::from_utf8_lossy(&payload[*pos..*pos + len]).into_owned();
    *pos += len;
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response::text_resultset;

    #[test]
    fn roundtrip_resultset() {
        let packets = text_resultset(
            &["id".into(), "name".into()],
            &[vec!["1".into(), "alice".into()]],
        );
        let first = classify_query_payload(&packets[0]).unwrap();
        let QueryResponse::Rows { columns, rows } = first else {
            panic!("expected partial rows state");
        };
        assert_eq!(columns.len(), 0);
        assert!(rows.is_empty());

        let col0 = column_name_from_definition(&packets[1]).unwrap();
        let col1 = column_name_from_definition(&packets[2]).unwrap();
        assert_eq!(col0, "id");
        assert_eq!(col1, "name");

        let row = decode_text_row(&packets[3]).unwrap();
        assert_eq!(row, vec!["1", "alice"]);
        assert_eq!(packets[4][0], 0xFE);
        assert_eq!(packets.len(), 5);
    }
}

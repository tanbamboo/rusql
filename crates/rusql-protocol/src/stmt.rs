//! Prepared statement wire packets (COM_STMT_*).

use crate::ProtocolError;

pub const COM_STMT_PREPARE: u8 = 0x16;
pub const COM_STMT_EXECUTE: u8 = 0x17;
pub const COM_STMT_CLOSE: u8 = 0x19;

const MYSQL_TYPE_LONGLONG: u8 = 0x08;
const MYSQL_TYPE_VAR_STRING: u8 = 0x0F;
const MYSQL_TYPE_STRING: u8 = 0xFE;

/// COM_STMT_PREPARE_OK payload.
pub fn stmt_prepare_ok(stmt_id: u32, num_columns: u16, num_params: u16) -> Vec<u8> {
    let mut p = Vec::with_capacity(12);
    p.push(0x00);
    p.extend_from_slice(&stmt_id.to_le_bytes());
    p.extend_from_slice(&num_columns.to_le_bytes());
    p.extend_from_slice(&num_params.to_le_bytes());
    p.push(0);
    p.extend_from_slice(&0u16.to_le_bytes());
    p
}

/// Column definition packet for prepared statement metadata.
pub fn stmt_field_definition(name: &str) -> Vec<u8> {
    crate::response::stmt_field_definition(name)
}

pub fn stmt_eof_packet() -> Vec<u8> {
    crate::response::stmt_eof_packet()
}

fn is_null(bitmap: &[u8], index: usize) -> bool {
    let byte = bitmap[index / 8];
    (byte & (1 << (index % 8))) != 0
}

fn read_lenenc_int(buf: &[u8]) -> Result<(u64, usize), ProtocolError> {
    if buf.is_empty() {
        return Err(ProtocolError::invalid_packet());
    }
    match buf[0] {
        n @ 0..=250 => Ok((n as u64, 1)),
        0xFC => {
            if buf.len() < 3 {
                return Err(ProtocolError::invalid_packet());
            }
            Ok((u16::from_le_bytes([buf[1], buf[2]]) as u64, 3))
        }
        0xFD => {
            if buf.len() < 4 {
                return Err(ProtocolError::invalid_packet());
            }
            let n = buf[1] as u64 | ((buf[2] as u64) << 8) | ((buf[3] as u64) << 16);
            Ok((n, 4))
        }
        0xFE => {
            if buf.len() < 9 {
                return Err(ProtocolError::invalid_packet());
            }
            Ok((u64::from_le_bytes(buf[1..9].try_into().unwrap()), 9))
        }
        _ => Err(ProtocolError::invalid_packet()),
    }
}

fn read_binary_param(buf: &[u8], col_type: u8) -> Result<(String, usize), ProtocolError> {
    match col_type {
        MYSQL_TYPE_LONGLONG => {
            if buf.len() < 8 {
                return Err(ProtocolError::invalid_packet());
            }
            let n = i64::from_le_bytes(buf[0..8].try_into().unwrap());
            Ok((n.to_string(), 8))
        }
        MYSQL_TYPE_VAR_STRING | MYSQL_TYPE_STRING => {
            let (len, hlen) = read_lenenc_int(buf)?;
            let start = hlen;
            let end = start + len as usize;
            if buf.len() < end {
                return Err(ProtocolError::invalid_packet());
            }
            let s = std::str::from_utf8(&buf[start..end])
                .map_err(|_| ProtocolError::invalid_packet())?;
            Ok((s.to_string(), end))
        }
        _ => Err(ProtocolError::Message(format!(
            "unsupported prepared param type: 0x{col_type:02X}"
        ))),
    }
}

/// Parse COM_STMT_EXECUTE payload into bound parameter values.
pub fn parse_stmt_execute(
    payload: &[u8],
    param_count: usize,
) -> Result<Vec<Option<String>>, ProtocolError> {
    if payload.first() != Some(&COM_STMT_EXECUTE) {
        return Err(ProtocolError::invalid_packet());
    }
    let mut pos = 1usize;
    if payload.len() < pos + 9 {
        return Err(ProtocolError::invalid_packet());
    }
    pos += 4; // statement_id
    pos += 1; // flags
    pos += 4; // iteration_count

    if param_count == 0 {
        return Ok(vec![]);
    }

    let bitmap_len = param_count.div_ceil(8);
    if payload.len() < pos + bitmap_len + 1 {
        return Err(ProtocolError::invalid_packet());
    }
    let null_bitmap = &payload[pos..pos + bitmap_len];
    pos += bitmap_len;
    let new_params = payload[pos];
    pos += 1;

    let mut types = vec![MYSQL_TYPE_VAR_STRING as u16; param_count];
    if new_params == 1 {
        if payload.len() < pos + param_count * 2 {
            return Err(ProtocolError::invalid_packet());
        }
        types.clear();
        for _ in 0..param_count {
            types.push(u16::from_le_bytes([payload[pos], payload[pos + 1]]));
            pos += 2;
        }
    }

    let mut out = Vec::with_capacity(param_count);
    for (i, ty) in types.iter().enumerate().take(param_count) {
        if is_null(null_bitmap, i) {
            out.push(None);
            continue;
        }
        let (val, n) = read_binary_param(&payload[pos..], *ty as u8)?;
        pos += n;
        out.push(Some(val));
    }
    Ok(out)
}

/// Build COM_STMT_EXECUTE for tests (text-oriented VARCHAR params).
pub fn encode_stmt_execute(stmt_id: u32, params: &[Option<String>]) -> Vec<u8> {
    let n = params.len();
    let mut p = Vec::new();
    p.push(COM_STMT_EXECUTE);
    p.extend_from_slice(&stmt_id.to_le_bytes());
    p.push(0);
    p.extend_from_slice(&1u32.to_le_bytes());

    let bitmap_len = n.div_ceil(8);
    let mut bitmap = vec![0u8; bitmap_len];
    for (i, param) in params.iter().enumerate() {
        if param.is_none() {
            bitmap[i / 8] |= 1 << (i % 8);
        }
    }
    p.extend_from_slice(&bitmap);
    p.push(1);
    for _ in 0..n {
        p.extend_from_slice(&[MYSQL_TYPE_VAR_STRING, 0]);
    }
    for s in params.iter().flatten() {
        write_lenenc_string(&mut p, s);
    }
    p
}

fn write_lenenc_string(buf: &mut Vec<u8>, s: &str) {
    write_lenenc_int(buf, s.len() as u64);
    buf.extend_from_slice(s.as_bytes());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_ok_layout() {
        let p = stmt_prepare_ok(7, 2, 1);
        assert_eq!(p[0], 0);
        assert_eq!(u32::from_le_bytes(p[1..5].try_into().unwrap()), 7);
    }

    #[test]
    fn execute_roundtrip() {
        let payload = encode_stmt_execute(1, &[Some("42".into()), None]);
        let params = parse_stmt_execute(&payload, 2).unwrap();
        assert_eq!(params[0], Some("42".into()));
        assert_eq!(params[1], None);
    }
}

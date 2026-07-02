//! MySQL binary protocol column types and resultset encoding.

pub const MYSQL_TYPE_DECIMAL: u8 = 0x00;
pub const MYSQL_TYPE_TINY: u8 = 0x01;
pub const MYSQL_TYPE_SHORT: u8 = 0x02;
pub const MYSQL_TYPE_LONG: u8 = 0x03;
pub const MYSQL_TYPE_FLOAT: u8 = 0x04;
pub const MYSQL_TYPE_DOUBLE: u8 = 0x05;
pub const MYSQL_TYPE_NULL: u8 = 0x06;
pub const MYSQL_TYPE_LONGLONG: u8 = 0x08;
pub const MYSQL_TYPE_INT24: u8 = 0x09;
pub const MYSQL_TYPE_YEAR: u8 = 0x0D;
pub const MYSQL_TYPE_VAR_STRING: u8 = 0x0F;
pub const MYSQL_TYPE_STRING: u8 = 0xFE;

/// Map rusql catalog / SQL type names to MySQL wire column types.
pub fn mysql_type_from_sql_type(data_type: &str) -> u8 {
    let upper = data_type.trim().to_uppercase();
    let base = upper.split('(').next().unwrap_or(upper.as_str()).trim();
    match base {
        "TINYINT" => MYSQL_TYPE_TINY,
        "SMALLINT" => MYSQL_TYPE_SHORT,
        "INT" | "INTEGER" | "MEDIUMINT" => MYSQL_TYPE_LONG,
        "BIGINT" => MYSQL_TYPE_LONGLONG,
        "FLOAT" => MYSQL_TYPE_FLOAT,
        "DOUBLE" => MYSQL_TYPE_DOUBLE,
        "VARCHAR" | "CHAR" | "TEXT" | "BLOB" | "JSON" => MYSQL_TYPE_VAR_STRING,
        _ => MYSQL_TYPE_VAR_STRING,
    }
}

/// Infer wire type for a prepared-statement result column name.
pub fn mysql_type_for_result_column(name: &str) -> u8 {
    if name.chars().all(|c| c.is_ascii_digit()) {
        MYSQL_TYPE_LONGLONG
    } else {
        MYSQL_TYPE_VAR_STRING
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

/// Treat empty strings as SQL NULL in binary rows (M21 NULL sentinel).
pub fn is_sql_null_value(value: &str) -> bool {
    value.is_empty()
}

/// Encode one cell for binary protocol result rows.
pub fn encode_binary_value(col_type: u8, value: &str) -> Vec<u8> {
    match col_type {
        MYSQL_TYPE_TINY => {
            let n: i8 = value.parse().unwrap_or(0);
            vec![n as u8]
        }
        MYSQL_TYPE_SHORT | MYSQL_TYPE_YEAR => {
            let n: i16 = value.parse().unwrap_or(0);
            n.to_le_bytes().to_vec()
        }
        MYSQL_TYPE_LONG | MYSQL_TYPE_INT24 => {
            let n: i32 = value.parse().unwrap_or(0);
            n.to_le_bytes().to_vec()
        }
        MYSQL_TYPE_LONGLONG => {
            let n: i64 = value.parse().unwrap_or(0);
            n.to_le_bytes().to_vec()
        }
        MYSQL_TYPE_FLOAT => {
            let n: f32 = value.parse().unwrap_or(0.0);
            n.to_le_bytes().to_vec()
        }
        MYSQL_TYPE_DOUBLE => {
            let n: f64 = value.parse().unwrap_or(0.0);
            n.to_le_bytes().to_vec()
        }
        MYSQL_TYPE_VAR_STRING | MYSQL_TYPE_STRING | MYSQL_TYPE_DECIMAL => {
            let mut buf = Vec::new();
            write_lenenc_string(&mut buf, value);
            buf
        }
        _ => {
            let mut buf = Vec::new();
            write_lenenc_string(&mut buf, value);
            buf
        }
    }
}

/// Build a binary resultset row for COM_STMT_EXECUTE (null bitmap offset 0).
pub fn binary_resultset_row(col_types: &[u8], values: &[String]) -> Vec<u8> {
    debug_assert_eq!(col_types.len(), values.len());
    let n = col_types.len();
    let bitmap_len = n.div_ceil(8);
    let mut bitmap = vec![0u8; bitmap_len];
    let mut encoded = Vec::new();

    for (i, (ty, val)) in col_types.iter().zip(values.iter()).enumerate() {
        if is_sql_null_value(val) {
            bitmap[i / 8] |= 1 << (i % 8);
        } else {
            encoded.extend_from_slice(&encode_binary_value(*ty, val));
        }
    }

    let mut row = Vec::with_capacity(1 + bitmap_len + encoded.len());
    row.push(0x00);
    row.extend_from_slice(&bitmap);
    row.extend(encoded);
    row
}

/// Decode a `COM_STMT_EXECUTE` binary row into string cells (for tests).
pub fn decode_binary_resultset_row(col_types: &[u8], payload: &[u8]) -> Option<Vec<String>> {
    if payload.first() != Some(&0x00) {
        return None;
    }
    let n = col_types.len();
    let bitmap_len = n.div_ceil(8);
    if payload.len() < 1 + bitmap_len {
        return None;
    }
    let bitmap = &payload[1..1 + bitmap_len];
    let mut pos = 1 + bitmap_len;
    let mut out = Vec::with_capacity(n);

    for (i, ty) in col_types.iter().enumerate() {
        if bitmap[i / 8] & (1 << (i % 8)) != 0 {
            out.push(String::new());
            continue;
        }
        let (val, consumed) = decode_binary_value(*ty, &payload[pos..])?;
        pos += consumed;
        out.push(val);
    }
    Some(out)
}

fn decode_binary_value(col_type: u8, buf: &[u8]) -> Option<(String, usize)> {
    match col_type {
        MYSQL_TYPE_TINY => {
            if buf.is_empty() {
                return None;
            }
            Some(((buf[0] as i8).to_string(), 1))
        }
        MYSQL_TYPE_SHORT | MYSQL_TYPE_YEAR => {
            if buf.len() < 2 {
                return None;
            }
            Some((i16::from_le_bytes([buf[0], buf[1]]).to_string(), 2))
        }
        MYSQL_TYPE_LONG | MYSQL_TYPE_INT24 => {
            if buf.len() < 4 {
                return None;
            }
            Some((
                i32::from_le_bytes(buf[0..4].try_into().ok()?).to_string(),
                4,
            ))
        }
        MYSQL_TYPE_LONGLONG => {
            if buf.len() < 8 {
                return None;
            }
            Some((
                i64::from_le_bytes(buf[0..8].try_into().ok()?).to_string(),
                8,
            ))
        }
        MYSQL_TYPE_FLOAT => {
            if buf.len() < 4 {
                return None;
            }
            Some((
                f32::from_le_bytes(buf[0..4].try_into().ok()?).to_string(),
                4,
            ))
        }
        MYSQL_TYPE_DOUBLE => {
            if buf.len() < 8 {
                return None;
            }
            Some((
                f64::from_le_bytes(buf[0..8].try_into().ok()?).to_string(),
                8,
            ))
        }
        MYSQL_TYPE_VAR_STRING | MYSQL_TYPE_STRING => {
            let len = read_lenenc_int(buf, &mut 0) as usize;
            let hlen = lenenc_header_len(buf[0]);
            if buf.len() < hlen + len {
                return None;
            }
            let s = String::from_utf8_lossy(&buf[hlen..hlen + len]).into_owned();
            Some((s, hlen + len))
        }
        _ => {
            let len = read_lenenc_int(buf, &mut 0) as usize;
            let hlen = lenenc_header_len(buf[0]);
            if buf.len() < hlen + len {
                return None;
            }
            let s = String::from_utf8_lossy(&buf[hlen..hlen + len]).into_owned();
            Some((s, hlen + len))
        }
    }
}

fn read_lenenc_int(buf: &[u8], pos: &mut usize) -> u64 {
    if *pos >= buf.len() {
        return 0;
    }
    let first = buf[*pos];
    *pos += 1;
    match first {
        n @ 0..=250 => n as u64,
        0xFC => {
            let v = u16::from_le_bytes([buf[*pos], buf[*pos + 1]]) as u64;
            *pos += 2;
            v
        }
        0xFD => {
            let v = u32::from_le_bytes([buf[*pos], buf[*pos + 1], buf[*pos + 2], 0]) as u64;
            *pos += 3;
            v
        }
        0xFE => {
            let mut b = [0u8; 8];
            b.copy_from_slice(&buf[*pos..*pos + 8]);
            *pos += 8;
            u64::from_le_bytes(b)
        }
        _ => 0,
    }
}

fn lenenc_header_len(first: u8) -> usize {
    match first {
        0..=250 => 1,
        0xFC => 3,
        0xFD => 4,
        0xFE => 9,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_type_mapping() {
        assert_eq!(mysql_type_from_sql_type("INT"), MYSQL_TYPE_LONG);
        assert_eq!(
            mysql_type_from_sql_type("VARCHAR(32)"),
            MYSQL_TYPE_VAR_STRING
        );
        assert_eq!(mysql_type_for_result_column("1"), MYSQL_TYPE_LONGLONG);
    }

    #[test]
    fn binary_row_int_roundtrip() {
        let types = [MYSQL_TYPE_LONG, MYSQL_TYPE_VAR_STRING];
        let values = vec!["42".into(), "hi".into()];
        let row = binary_resultset_row(&types, &values);
        let decoded = decode_binary_resultset_row(&types, &row).unwrap();
        assert_eq!(decoded, values);
    }

    #[test]
    fn binary_row_null_int() {
        let types = [MYSQL_TYPE_LONG];
        let values = vec![String::new()];
        let row = binary_resultset_row(&types, &values);
        let decoded = decode_binary_resultset_row(&types, &row).unwrap();
        assert_eq!(decoded, values);
    }
}

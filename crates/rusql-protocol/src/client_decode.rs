//! Decode MySQL text protocol responses (for tests and compat harness).

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

/// Decode a text result row packet.
pub fn decode_text_row(payload: &[u8]) -> Option<Vec<String>> {
    let mut pos = 0;
    let mut values = Vec::new();
    while pos < payload.len() {
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
    }
}

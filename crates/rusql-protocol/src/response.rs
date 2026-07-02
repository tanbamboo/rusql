//! MySQL server response packets (OK, ERR, resultset).

use crate::binary::{binary_resultset_row, MYSQL_TYPE_VAR_STRING};
use crate::handshake::{encode_err_payload, encode_ok_payload};

const SERVER_STATUS_AUTOCOMMIT: u16 = 0x0002;
const EOF_MARKER: u8 = 0xFE;

/// Build OK packet with affected rows / last insert id.
pub fn ok_packet(affected_rows: u64, last_insert_id: u64) -> Vec<u8> {
    let mut payload = encode_ok_payload();
    payload[1] = lenenc_byte(affected_rows);
    payload[2] = lenenc_byte(last_insert_id);
    payload
}

fn lenenc_byte(n: u64) -> u8 {
    debug_assert!(n < 251);
    n as u8
}

/// Build OK packet with custom affected row counts (multi-byte lenenc).
pub fn ok_packet_full(affected_rows: u64, last_insert_id: u64) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.push(0x00);
    write_lenenc_int(&mut payload, affected_rows);
    write_lenenc_int(&mut payload, last_insert_id);
    payload.extend_from_slice(&SERVER_STATUS_AUTOCOMMIT.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload
}

/// Build ERR packet.
pub fn err_packet(code: u16, message: &str) -> Vec<u8> {
    encode_err_payload(code, message)
}

/// Build all payloads for a text resultset (column_count, coldefs, rows, EOF).
pub fn text_resultset(columns: &[String], rows: &[Vec<String>]) -> Vec<Vec<u8>> {
    let types: Vec<u8> = columns.iter().map(|_| MYSQL_TYPE_VAR_STRING).collect();
    text_resultset_typed(columns, &types, rows)
}

/// Build all payloads for a binary `COM_STMT_EXECUTE` resultset.
pub fn binary_resultset(
    columns: &[String],
    col_types: &[u8],
    rows: &[Vec<String>],
) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();

    let mut col_count = Vec::new();
    write_lenenc_int(&mut col_count, columns.len() as u64);
    packets.push(col_count);

    for (name, ty) in columns.iter().zip(col_types.iter()) {
        packets.push(column_definition(name, *ty));
    }

    for row in rows {
        packets.push(binary_resultset_row(col_types, row));
    }

    packets.push(eof_packet());
    packets
}

fn text_resultset_typed(
    columns: &[String],
    col_types: &[u8],
    rows: &[Vec<String>],
) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();

    let mut col_count = Vec::new();
    write_lenenc_int(&mut col_count, columns.len() as u64);
    packets.push(col_count);

    for (col, ty) in columns.iter().zip(col_types.iter()) {
        packets.push(column_definition(col, *ty));
    }

    for row in rows {
        packets.push(text_row(row));
    }

    packets.push(eof_packet());
    packets
}

fn column_definition(name: &str, mysql_type: u8) -> Vec<u8> {
    let mut p = Vec::new();
    write_lenenc_string(&mut p, "def");
    write_lenenc_string(&mut p, "");
    write_lenenc_string(&mut p, "");
    write_lenenc_string(&mut p, "");
    write_lenenc_string(&mut p, name);
    write_lenenc_string(&mut p, name);
    p.push(0x0c);
    p.extend_from_slice(&0x0021u16.to_le_bytes());
    p.extend_from_slice(&64u32.to_le_bytes());
    p.push(mysql_type);
    p.extend_from_slice(&0u16.to_le_bytes());
    p.push(0);
    p.extend_from_slice(&[0u8; 2]);
    p
}

fn text_row(values: &[String]) -> Vec<u8> {
    let mut p = Vec::new();
    for v in values {
        write_lenenc_string(&mut p, v);
    }
    p
}

fn eof_packet() -> Vec<u8> {
    let mut p = Vec::new();
    p.push(EOF_MARKER);
    p.extend_from_slice(&0u16.to_le_bytes());
    p.extend_from_slice(&SERVER_STATUS_AUTOCOMMIT.to_le_bytes());
    p
}

/// EOF after prepared-statement column/param definitions.
pub fn stmt_eof_packet() -> Vec<u8> {
    eof_packet()
}

/// Column definition for COM_STMT_PREPARE metadata.
pub fn stmt_field_definition(name: &str, mysql_type: u8) -> Vec<u8> {
    column_definition(name, mysql_type)
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
    fn ok_packet_starts_with_zero() {
        assert_eq!(ok_packet_full(0, 0)[0], 0x00);
    }

    #[test]
    fn resultset_structure() {
        let packets = text_resultset(&["id".into()], &[vec!["1".into()]]);
        assert_eq!(packets.len(), 4);
        assert_eq!(packets[0][0], 1);
        assert_eq!(packets.last().unwrap()[0], EOF_MARKER);
    }

    #[test]
    fn binary_resultset_structure() {
        use crate::binary::MYSQL_TYPE_LONG;
        let packets = binary_resultset(&["id".into()], &[MYSQL_TYPE_LONG], &[vec!["1".into()]]);
        assert_eq!(packets.len(), 4);
        assert_eq!(packets[2][0], 0x00);
        assert_eq!(packets.last().unwrap()[0], EOF_MARKER);
    }
}

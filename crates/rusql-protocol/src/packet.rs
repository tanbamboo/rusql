//! MySQL packet framing: 3-byte little-endian length + 1-byte sequence + payload.

use crate::ProtocolError;

pub const MAX_PACKET_SIZE: usize = 16_777_215;

/// Raw MySQL packet view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub sequence: u8,
    pub payload: Vec<u8>,
}

impl Packet {
    pub fn new(sequence: u8, payload: impl Into<Vec<u8>>) -> Self {
        Self {
            sequence,
            payload: payload.into(),
        }
    }

    pub fn decode(buf: &[u8]) -> Result<(u8, Vec<u8>), ProtocolError> {
        if buf.len() < 4 {
            return Err(ProtocolError::InvalidLength(buf.len()));
        }
        let len = u32::from_le_bytes([buf[0], buf[1], buf[2], 0]) as usize;
        if buf.len() < 4 + len {
            return Err(ProtocolError::InvalidLength(buf.len()));
        }
        let sequence = buf[3];
        let payload = buf[4..4 + len].to_vec();
        Ok((sequence, payload))
    }
}

/// Encode MySQL packets.
pub struct PacketWriter;

impl PacketWriter {
    pub fn encode(sequence: u8, payload: &[u8]) -> Vec<u8> {
        let len = payload.len();
        assert!(len <= MAX_PACKET_SIZE);
        let mut buf = Vec::with_capacity(4 + len);
        buf.push((len & 0xFF) as u8);
        buf.push(((len >> 8) & 0xFF) as u8);
        buf.push(((len >> 16) & 0xFF) as u8);
        buf.push(sequence);
        buf.extend_from_slice(payload);
        buf
    }
}

/// Read MySQL packets from a byte stream (incremental).
#[derive(Debug, Default)]
pub struct PacketReader {
    buffer: Vec<u8>,
}

impl PacketReader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    pub fn next_packet(&mut self) -> Result<Option<Packet>, ProtocolError> {
        if self.buffer.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_le_bytes([
            self.buffer[0],
            self.buffer[1],
            self.buffer[2],
            0,
        ]) as usize;
        if len > MAX_PACKET_SIZE {
            return Err(ProtocolError::PacketTooLarge);
        }
        if self.buffer.len() < 4 + len {
            return Ok(None);
        }
        let sequence = self.buffer[3];
        let payload = self.buffer[4..4 + len].to_vec();
        self.buffer.drain(..4 + len);
        Ok(Some(Packet { sequence, payload }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_incremental() {
        let encoded = PacketWriter::encode(1, b"ping");
        let mut reader = PacketReader::new();
        reader.push(&encoded[..2]);
        assert!(reader.next_packet().unwrap().is_none());
        reader.push(&encoded[2..]);
        let pkt = reader.next_packet().unwrap().unwrap();
        assert_eq!(pkt.sequence, 1);
        assert_eq!(pkt.payload, b"ping");
    }
}

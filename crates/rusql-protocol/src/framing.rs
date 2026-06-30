//! MySQL packet framing over async streams.

use crate::packet::PacketWriter;
use crate::ProtocolError;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Read one framed packet; returns (sequence_id, payload).
pub async fn read_packet<S>(stream: &mut S) -> Result<(u8, Vec<u8>), ProtocolError>
where
    S: AsyncRead + Unpin,
{
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|_| ProtocolError::invalid_packet())?;
    let len = u32::from_le_bytes([header[0], header[1], header[2], 0]) as usize;
    let seq = header[3];
    let mut payload = vec![0u8; len];
    if len > 0 {
        stream
            .read_exact(&mut payload)
            .await
            .map_err(|_| ProtocolError::invalid_packet())?;
    }
    Ok((seq, payload))
}

/// Read packet and verify sequence id.
pub async fn read_packet_seq<S>(stream: &mut S, expected_seq: u8) -> Result<Vec<u8>, ProtocolError>
where
    S: AsyncRead + Unpin,
{
    let (seq, payload) = read_packet(stream).await?;
    if seq != expected_seq {
        return Err(ProtocolError::handshake_failed());
    }
    Ok(payload)
}

/// Write one framed packet.
pub async fn write_packet<S>(stream: &mut S, seq: u8, payload: &[u8]) -> Result<(), ProtocolError>
where
    S: AsyncWrite + Unpin,
{
    let framed = PacketWriter::encode(seq, payload);
    stream
        .write_all(&framed)
        .await
        .map_err(|_| ProtocolError::invalid_packet())?;
    stream
        .flush()
        .await
        .map_err(|_| ProtocolError::invalid_packet())?;
    Ok(())
}

/// Write multiple response payloads with incrementing sequence numbers.
pub async fn write_packets<S>(
    stream: &mut S,
    start_seq: u8,
    payloads: &[Vec<u8>],
) -> Result<(), ProtocolError>
where
    S: AsyncWrite + Unpin,
{
    for (i, payload) in payloads.iter().enumerate() {
        write_packet(stream, start_seq.wrapping_add(i as u8), payload).await?;
    }
    Ok(())
}

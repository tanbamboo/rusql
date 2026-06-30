//! MySQL command-phase client commands.

use crate::ProtocolError;

pub const COM_QUIT: u8 = 0x01;
pub const COM_QUERY: u8 = 0x03;

/// Parsed client command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientCommand {
    Quit,
    Query(String),
    Unknown(u8),
}

/// Parse COM_* command payload (first byte is command code).
pub fn parse_command(payload: &[u8]) -> Result<ClientCommand, ProtocolError> {
    if payload.is_empty() {
        return Err(ProtocolError::invalid_packet());
    }
    match payload[0] {
        COM_QUIT => Ok(ClientCommand::Quit),
        COM_QUERY => {
            let sql = std::str::from_utf8(&payload[1..])
                .map_err(|_| ProtocolError::invalid_packet())?
                .trim_end_matches('\0')
                .to_string();
            Ok(ClientCommand::Query(sql))
        }
        other => Ok(ClientCommand::Unknown(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_com_query() {
        let mut p = vec![COM_QUERY];
        p.extend_from_slice(b"SELECT 1");
        let cmd = parse_command(&p).unwrap();
        assert_eq!(cmd, ClientCommand::Query("SELECT 1".into()));
    }
}

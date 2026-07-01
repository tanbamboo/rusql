//! MySQL command-phase client commands.

use crate::ProtocolError;

pub const COM_QUIT: u8 = 0x01;
pub const COM_QUERY: u8 = 0x03;
pub const COM_STMT_PREPARE: u8 = 0x16;
pub const COM_STMT_EXECUTE: u8 = 0x17;
pub const COM_STMT_CLOSE: u8 = 0x19;

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

//! Per-connection command loop after handshake.

use rusql_core::Session;
use rusql_executor::{heap_executor, ExecError, Executor, QueryResult};
use rusql_planner::plan;
use rusql_protocol::{
    err_packet, ok_packet_full, parse_command, read_packet, server_handshake, text_resultset,
    write_packets, ClientCommand, HandshakeConfig, HandshakeSession, ProtocolError,
};
use rusql_sql::parse;
use rusql_storage::HeapEngine;
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{debug, warn};

/// Run handshake then process COM_* commands until QUIT or disconnect.
pub async fn serve_connection<S>(
    stream: &mut S,
    config: &HandshakeConfig,
    connection_id: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let hs = server_handshake(stream, config, connection_id).await?;
    run_command_loop(stream, hs).await
}

async fn run_command_loop<S>(stream: &mut S, hs: HandshakeSession) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut session = Session::new(hs.connection_id as u64, hs.username);
    if let Some(db) = hs.database {
        session.user.push_str(&format!("@{db}"));
    }
    let mut executor: Executor<HeapEngine> = heap_executor();

    loop {
        let (_seq, payload) = read_packet(stream).await?;
        match parse_command(&payload)? {
            ClientCommand::Quit => {
                debug!(connection_id = hs.connection_id, "client quit");
                break;
            }
            ClientCommand::Query(sql) => {
                debug!(connection_id = hs.connection_id, %sql, "com_query");
                if let Err(e) = execute_sql(stream, &mut session, &mut executor, &sql).await {
                    warn!(connection_id = hs.connection_id, error = %e, "query failed");
                }
            }
            ClientCommand::Unknown(code) => {
                let msg = format!("unsupported command: 0x{code:02X}");
                let err = err_packet(1047, &msg);
                write_packets(stream, 1, &[err]).await?;
            }
        }
    }
    Ok(())
}

async fn execute_sql<S>(
    stream: &mut S,
    session: &mut Session,
    executor: &mut Executor<HeapEngine>,
    sql: &str,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let statements = match parse(sql) {
        Ok(s) => s,
        Err(e) => {
            let err = err_packet(1064, &e.to_string());
            write_packets(stream, 1, &[err]).await?;
            return Ok(());
        }
    };

    let plans = plan(session, statements);
    let results = match executor.execute(session, &plans) {
        Ok(r) => r,
        Err(ExecError::Message(m)) => {
            let err = err_packet(1105, &m);
            write_packets(stream, 1, &[err]).await?;
            return Ok(());
        }
        Err(ExecError::Storage(e)) => {
            let err = err_packet(1146, &e.to_string());
            write_packets(stream, 1, &[err]).await?;
            return Ok(());
        }
    };

    let mut seq = 1u8;
    for result in results {
        match result {
            QueryResult::Ok { rows_affected } => {
                let ok = ok_packet_full(rows_affected, 0);
                write_packets(stream, seq, &[ok]).await?;
                seq = seq.wrapping_add(1);
            }
            QueryResult::Rows { columns, rows } => {
                let payloads = text_resultset(&columns, &rows);
                write_packets(stream, seq, &payloads).await?;
                seq = seq.wrapping_add(payloads.len() as u8);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_protocol::handshake::{HandshakeResponse, InitialHandshake};
    use rusql_protocol::{write_packet, PacketWriter, COM_QUERY};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    const CLIENT_PROTOCOL_41: u32 = 0x0000_0200;
    const CLIENT_PLUGIN_AUTH: u32 = 0x0008_0000;
    const CLIENT_SECURE_CONNECTION: u32 = 0x0000_8000;
    const CLIENT_PLUGIN_AUTH_LENENC: u32 = 0x0020_0000;

    async fn client_handshake(stream: &mut TcpStream) {
        let mut hdr = [0u8; 4];
        stream.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await.unwrap();
        InitialHandshake::decode_payload(&payload).unwrap();

        let response = HandshakeResponse {
            capabilities: CLIENT_PROTOCOL_41
                | CLIENT_PLUGIN_AUTH
                | CLIENT_SECURE_CONNECTION
                | CLIENT_PLUGIN_AUTH_LENENC,
            username: "root".into(),
            auth_response: vec![0],
            database: None,
            auth_plugin: Some("mysql_native_password".into()),
        };
        stream
            .write_all(&PacketWriter::encode(1, &response.encode_payload()))
            .await
            .unwrap();

        stream.read_exact(&mut hdr).await.unwrap();
        let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
        payload.resize(len, 0);
        stream.read_exact(&mut payload).await.unwrap();
        assert_eq!(payload[0], 0x00);
    }

    async fn client_query(stream: &mut TcpStream, sql: &str) -> Vec<u8> {
        let mut cmd = vec![COM_QUERY];
        cmd.extend_from_slice(sql.as_bytes());
        write_packet(stream, 0, &cmd).await.unwrap();
        let (_seq, payload) = read_packet(stream).await.unwrap();
        payload
    }

    #[tokio::test]
    async fn com_query_create_insert_select() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            serve_connection(&mut stream, &HandshakeConfig::default(), 1)
                .await
                .unwrap();
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        client_handshake(&mut client).await;

        let ok = client_query(&mut client, "CREATE TABLE t (id INT)").await;
        assert_eq!(ok[0], 0x00);

        let ok = client_query(&mut client, "INSERT INTO t VALUES (1)").await;
        assert_eq!(ok[0], 0x00);

        let first = client_query(&mut client, "SELECT * FROM t").await;
        assert_eq!(first[0], 1);

        let (_s, _coldef) = read_packet(&mut client).await.unwrap();
        let (_s, row) = read_packet(&mut client).await.unwrap();
        assert!(!row.is_empty());
        let (_s, eof) = read_packet(&mut client).await.unwrap();
        assert_eq!(eof[0], 0xFE);

        write_packet(&mut client, 0, &[0x01]).await.unwrap();
        server.await.unwrap();
    }
}

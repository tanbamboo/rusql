//! Per-connection command loop after handshake.

use rusql_core::Session;
use rusql_executor::{execute, ExecError, QueryResult};
use rusql_planner::plan;
use rusql_protocol::{
    err_packet, ok_packet_full, parse_command, read_packet, server_handshake, text_resultset,
    write_packets, ClientCommand, HandshakeConfig, HandshakeSession, ProtocolError,
};
use rusql_sql::parse;
use rusql_storage::PersistentEngine;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::Mutex;
use tracing::{debug, warn};

/// Run handshake then process COM_* commands until QUIT or disconnect.
pub async fn serve_connection<S>(
    stream: &mut S,
    config: &HandshakeConfig,
    connection_id: u32,
    engine: Arc<Mutex<PersistentEngine>>,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let hs = server_handshake(stream, config, connection_id).await?;
    run_command_loop(stream, hs, engine).await
}

async fn run_command_loop<S>(
    stream: &mut S,
    hs: HandshakeSession,
    engine: Arc<Mutex<PersistentEngine>>,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut session = Session::new(hs.connection_id as u64, hs.username.clone());
    if let Some(db) = hs.database {
        session.user.push_str(&format!("@{db}"));
    }
    seed_session_catalog(&mut session, &engine).await;

    loop {
        let (_seq, payload) = read_packet(stream).await?;
        match parse_command(&payload)? {
            ClientCommand::Quit => {
                debug!(connection_id = hs.connection_id, "client quit");
                break;
            }
            ClientCommand::Query(sql) => {
                debug!(connection_id = hs.connection_id, %sql, "com_query");
                if let Err(e) = execute_sql(stream, &mut session, &engine, &sql).await {
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

async fn seed_session_catalog(session: &mut Session, engine: &Arc<Mutex<PersistentEngine>>) {
    let eng = engine.lock().await;
    for meta in eng.table_metas() {
        session.catalog.create_table(meta);
    }
}

async fn execute_sql<S>(
    stream: &mut S,
    session: &mut Session,
    engine: &Arc<Mutex<PersistentEngine>>,
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
    let results = {
        let mut eng = engine.lock().await;
        match execute(&mut *eng, session, &plans) {
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
    use crate::test_support::TestServer;
    use rusql_protocol::client_decode::QueryResponse;
    use rusql_storage::{PersistentEngine, StorageEngine};

    #[tokio::test]
    async fn com_query_create_insert_select() {
        let server = TestServer::start("com_query").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client.query("CREATE TABLE t (id INT)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO t VALUES (1)").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT * FROM t").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["id".to_string()]);
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected rows, got {other:?}"),
        }
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn persistence_across_connections() {
        let server = TestServer::start("persist_conn").await;

        let mut c1 = server.connect().await;
        assert!(matches!(
            c1.query("CREATE TABLE items (id INT)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            c1.query("INSERT INTO items VALUES (99)").await,
            QueryResponse::Ok { .. }
        ));
        c1.quit().await;

        let mut c2 = server.connect().await;
        match c2.query("SELECT * FROM items").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["99".to_string()]]);
            }
            other => panic!("expected rows, got {other:?}"),
        }
        c2.quit().await;

        let eng = PersistentEngine::open(&server.data_dir).unwrap();
        assert_eq!(eng.scan("items").unwrap(), vec![vec!["99".to_string()]]);

        let _ = std::fs::remove_dir_all(&server.data_dir);
    }
}

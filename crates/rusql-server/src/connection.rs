//! Per-connection command loop after handshake.

use crate::prepared::PreparedStatementStore;
use rusql_core::Session;
use rusql_executor::{execute, ExecError, QueryResult};
use rusql_planner::plan;
use rusql_protocol::{
    err_packet, ok_packet_full, parse_command, parse_stmt_execute, read_packet, server_handshake,
    stmt_eof_packet, stmt_field_definition, stmt_prepare_ok, text_resultset, write_packets,
    ClientCommand, HandshakeConfig, HandshakeSession, ProtocolError,
};
use rusql_sql::parse;
use rusql_storage::{OverlayEngine, PersistentEngine, TransactionState};
use sqlparser::ast::Statement;
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
    let mut txn: Option<TransactionState> = None;
    let mut stmts = PreparedStatementStore::new();

    loop {
        let (_seq, payload) = read_packet(stream).await?;
        match parse_command(&payload)? {
            ClientCommand::Quit => {
                debug!(connection_id = hs.connection_id, "client quit");
                break;
            }
            ClientCommand::Query(sql) => {
                debug!(connection_id = hs.connection_id, %sql, "com_query");
                if let Err(e) = execute_sql(stream, &mut session, &engine, &mut txn, &sql, 1).await
                {
                    warn!(connection_id = hs.connection_id, error = %e, "query failed");
                }
            }
            ClientCommand::StmtPrepare(sql) => {
                debug!(connection_id = hs.connection_id, %sql, "com_stmt_prepare");
                if let Err(e) = handle_stmt_prepare(stream, &session, &mut stmts, &sql).await {
                    warn!(connection_id = hs.connection_id, error = %e, "stmt prepare failed");
                }
            }
            ClientCommand::StmtExecute { stmt_id, payload } => {
                debug!(
                    connection_id = hs.connection_id,
                    stmt_id, "com_stmt_execute"
                );
                if let Err(e) = handle_stmt_execute(
                    stream,
                    &mut session,
                    &engine,
                    &mut txn,
                    &stmts,
                    stmt_id,
                    &payload,
                )
                .await
                {
                    warn!(connection_id = hs.connection_id, error = %e, "stmt execute failed");
                }
            }
            ClientCommand::StmtClose { stmt_id } => {
                stmts.close(stmt_id);
                let ok = ok_packet_full(0, 0);
                write_packets(stream, 1, &[ok]).await?;
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

async fn handle_stmt_prepare<S>(
    stream: &mut S,
    session: &Session,
    store: &mut PreparedStatementStore,
    sql: &str,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (stmt_id, stmt) = match store.prepare(session, sql.to_string()) {
        Ok(v) => v,
        Err(e) => {
            let err = err_packet(1064, &e);
            write_packets(stream, 1, &[err]).await?;
            return Ok(());
        }
    };
    let mut packets = vec![stmt_prepare_ok(
        stmt_id,
        stmt.result_columns.len() as u16,
        stmt.param_count as u16,
    )];
    for _ in 0..stmt.param_count {
        packets.push(stmt_field_definition("?"));
    }
    if stmt.param_count > 0 {
        packets.push(stmt_eof_packet());
    }
    for col in &stmt.result_columns {
        packets.push(stmt_field_definition(col));
    }
    if !stmt.result_columns.is_empty() {
        packets.push(stmt_eof_packet());
    }
    write_packets(stream, 1, &packets).await?;
    Ok(())
}

async fn handle_stmt_execute<S>(
    stream: &mut S,
    session: &mut Session,
    engine: &Arc<Mutex<PersistentEngine>>,
    txn: &mut Option<TransactionState>,
    store: &PreparedStatementStore,
    stmt_id: u32,
    payload: &[u8],
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Some(stmt) = store.get(stmt_id) else {
        let err = err_packet(1210, "unknown prepared statement handler");
        write_packets(stream, 1, &[err]).await?;
        return Ok(());
    };
    let params = match parse_stmt_execute(payload, stmt.param_count) {
        Ok(p) => p,
        Err(e) => {
            let err = err_packet(1105, &e.to_string());
            write_packets(stream, 1, &[err]).await?;
            return Ok(());
        }
    };
    let sql = match store.bound_sql(stmt_id, &params) {
        Ok(s) => s,
        Err(e) => {
            let err = err_packet(1064, &e);
            write_packets(stream, 1, &[err]).await?;
            return Ok(());
        }
    };
    execute_sql(stream, session, engine, txn, &sql, 1).await
}

async fn execute_sql<S>(
    stream: &mut S,
    session: &mut Session,
    engine: &Arc<Mutex<PersistentEngine>>,
    txn: &mut Option<TransactionState>,
    sql: &str,
    seq_start: u8,
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

    let mut all_results = Vec::new();
    for stmt in statements {
        match stmt {
            Statement::StartTransaction { .. } => {
                if txn.is_some() {
                    let err = err_packet(1105, "transaction already active");
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                *txn = Some(TransactionState::new());
                all_results.push(QueryResult::Ok { rows_affected: 0 });
            }
            Statement::Commit { .. } => {
                let Some(state) = txn.take() else {
                    let err = err_packet(1105, "no active transaction");
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                };
                let mut eng = engine.lock().await;
                if let Err(e) = eng.commit_transaction(state.pending_records()) {
                    let err = err_packet(1105, &e.to_string());
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                drop(eng);
                seed_session_catalog(session, engine).await;
                all_results.push(QueryResult::Ok { rows_affected: 0 });
            }
            Statement::Rollback { .. } => {
                if txn.take().is_none() {
                    let err = err_packet(1105, "no active transaction");
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                all_results.push(QueryResult::Ok { rows_affected: 0 });
            }
            other => {
                let plans = plan(session, vec![other]);
                let results = {
                    let mut eng = engine.lock().await;
                    match txn {
                        Some(ref mut t) => {
                            let mut overlay = OverlayEngine::new(&eng, t);
                            execute(&mut overlay, session, &plans)
                        }
                        None => execute(&mut *eng, session, &plans),
                    }
                };
                match results {
                    Ok(r) => all_results.extend(r),
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
            }
        }
    }

    let mut seq = seq_start;
    for result in all_results {
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
mod auth_tests {
    use crate::test_support::TestServer;
    use rusql_protocol::{AuthCredentials, HandshakeConfig};

    #[tokio::test]
    async fn rejects_wrong_password_when_auth_enabled() {
        let cfg = HandshakeConfig {
            auth_credentials: Some(AuthCredentials {
                username: "root".into(),
                password: "secret".into(),
            }),
            ..Default::default()
        };
        let server = TestServer::start_with_handshake("auth_fail", cfg).await;
        assert_eq!(
            server.try_connect_as("root", "wrong").await.unwrap_err(),
            0xFF
        );
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn accepts_caching_sha2_password_when_auth_enabled() {
        let cfg = HandshakeConfig {
            auth_credentials: Some(AuthCredentials {
                username: "root".into(),
                password: "secret".into(),
            }),
            ..Default::default()
        };
        let server = TestServer::start_with_handshake("auth_sha2", cfg).await;
        let mut client = server.connect_as("root", "secret").await;
        assert!(matches!(
            client.query("SELECT 1").await,
            rusql_protocol::client_decode::QueryResponse::Rows { .. }
        ));
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }
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

    #[tokio::test]
    async fn transaction_commit_and_rollback() {
        let server = TestServer::start("txn").await;

        let mut c1 = server.connect().await;
        assert!(matches!(
            c1.query("CREATE TABLE tx (id INT)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(c1.query("BEGIN").await, QueryResponse::Ok { .. }));
        assert!(matches!(
            c1.query("INSERT INTO tx VALUES (1)").await,
            QueryResponse::Ok { .. }
        ));
        match c1.query("SELECT * FROM tx").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected rows in txn, got {other:?}"),
        }

        let mut c2 = server.connect().await;
        match c2.query("SELECT * FROM tx").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("other connection should not see uncommitted rows: {other:?}"),
        }
        c2.quit().await;

        assert!(matches!(
            c1.query("ROLLBACK").await,
            QueryResponse::Ok { .. }
        ));
        match c1.query("SELECT * FROM tx").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty after rollback, got {other:?}"),
        }

        assert!(matches!(c1.query("BEGIN").await, QueryResponse::Ok { .. }));
        assert!(matches!(
            c1.query("INSERT INTO tx VALUES (2)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(c1.query("COMMIT").await, QueryResponse::Ok { .. }));
        c1.quit().await;

        let mut c3 = server.connect().await;
        match c3.query("SELECT * FROM tx").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["2".to_string()]]);
            }
            other => panic!("expected committed row, got {other:?}"),
        }
        c3.quit().await;

        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn stmt_prepare_execute_select() {
        let server = TestServer::start("stmt").await;
        let mut client = server.connect().await;

        let stmt_id = client.stmt_prepare("SELECT 1").await;
        assert_eq!(stmt_id, 1);
        match client.stmt_execute(stmt_id, &[]).await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["1".to_string()]);
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn stmt_prepare_execute_insert_param() {
        let server = TestServer::start("stmt_param").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client.query("CREATE TABLE p (id INT)").await,
            QueryResponse::Ok { .. }
        ));
        let stmt_id = client.stmt_prepare("INSERT INTO p VALUES (?)").await;
        assert!(matches!(
            client.stmt_execute(stmt_id, &[Some("7".into())]).await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT * FROM p").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["7".to_string()]]);
            }
            other => panic!("expected row, got {other:?}"),
        }
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn describe_and_information_schema() {
        let server = TestServer::start("describe").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE TABLE meta (id INT, name VARCHAR(32))")
                .await,
            QueryResponse::Ok { .. }
        ));

        match client.query("DESCRIBE meta").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Field");
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0][0], "id");
            }
            other => panic!("expected describe rows, got {other:?}"),
        }

        match client
            .query("SELECT * FROM information_schema.tables")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows
                    .iter()
                    .any(|r| r.get(1).map(|s| s.as_str()) == Some("meta")));
            }
            other => panic!("expected info_schema tables, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn use_database_ok() {
        let server = TestServer::start("use_db").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client.query("USE rusql").await,
            QueryResponse::Ok { .. }
        ));

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }
}

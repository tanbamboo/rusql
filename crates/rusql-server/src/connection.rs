//! Per-connection command loop after handshake.

use crate::prepared::PreparedStatementStore;
use rusql_core::Session;
use rusql_executor::{execute, ExecError, QueryResult};
use rusql_planner::plan;
use rusql_protocol::{
    binary_resultset_for_client, err_packet, ok_packet_for_client, parse_command,
    parse_stmt_execute, read_packet, server_handshake, stmt_eof_packet_for_client,
    stmt_field_definition, stmt_prepare_ok, text_resultset_for_client, write_packets,
    ClientCommand, HandshakeConfig, HandshakeSession, ProtocolError, MYSQL_TYPE_VAR_STRING,
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
        let cmd = match parse_command(&payload, hs.client_capabilities) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("protocol parse error: {e}");
                let err = err_packet(1064, &msg);
                write_packets(stream, 1, &[err]).await?;
                continue;
            }
        };
        match cmd {
            ClientCommand::Quit => {
                debug!(connection_id = hs.connection_id, "client quit");
                break;
            }
            ClientCommand::Query(sql) => {
                debug!(connection_id = hs.connection_id, %sql, "com_query");
                if let Err(e) = execute_sql(
                    stream,
                    &mut session,
                    &engine,
                    &mut txn,
                    &sql,
                    1,
                    None,
                    hs.client_capabilities,
                )
                .await
                {
                    warn!(connection_id = hs.connection_id, error = %e, "query failed");
                }
            }
            ClientCommand::StmtPrepare(sql) => {
                debug!(connection_id = hs.connection_id, %sql, "com_stmt_prepare");
                if let Err(e) =
                    handle_stmt_prepare(stream, &session, &mut stmts, &sql, hs.client_capabilities)
                        .await
                {
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
                    hs.client_capabilities,
                )
                .await
                {
                    warn!(connection_id = hs.connection_id, error = %e, "stmt execute failed");
                }
            }
            ClientCommand::StmtClose { stmt_id } => {
                stmts.close(stmt_id);
                let ok = ok_packet_for_client(0, 0, hs.client_capabilities);
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
    client_caps: u32,
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
        packets.push(stmt_field_definition("?", MYSQL_TYPE_VAR_STRING));
    }
    if stmt.param_count > 0 {
        packets.push(stmt_eof_packet_for_client(client_caps));
    }
    for (col, ty) in stmt
        .result_columns
        .iter()
        .zip(stmt.result_column_types.iter())
    {
        packets.push(stmt_field_definition(col, *ty));
    }
    if !stmt.result_columns.is_empty() {
        packets.push(stmt_eof_packet_for_client(client_caps));
    }
    write_packets(stream, 1, &packets).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_stmt_execute<S>(
    stream: &mut S,
    session: &mut Session,
    engine: &Arc<Mutex<PersistentEngine>>,
    txn: &mut Option<TransactionState>,
    store: &PreparedStatementStore,
    stmt_id: u32,
    payload: &[u8],
    client_caps: u32,
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
    let binary_types = if stmt.result_columns.is_empty() {
        None
    } else {
        Some(stmt.result_column_types.as_slice())
    };
    execute_sql(
        stream,
        session,
        engine,
        txn,
        &sql,
        1,
        binary_types,
        client_caps,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn execute_sql<S>(
    stream: &mut S,
    session: &mut Session,
    engine: &Arc<Mutex<PersistentEngine>>,
    txn: &mut Option<TransactionState>,
    sql: &str,
    seq_start: u8,
    binary_column_types: Option<&[u8]>,
    client_caps: u32,
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
                let ok = ok_packet_for_client(rows_affected, 0, client_caps);
                write_packets(stream, seq, &[ok]).await?;
                seq = seq.wrapping_add(1);
            }
            QueryResult::Rows { columns, rows } => {
                let payloads = if let Some(types) = binary_column_types {
                    binary_resultset_for_client(&columns, types, &rows, client_caps)
                } else {
                    text_resultset_for_client(&columns, &rows, client_caps)
                };
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
        let mut cfg = HandshakeConfig {
            auth_credentials: Some(AuthCredentials {
                username: "root".into(),
                password: "secret".into(),
            }),
            ..Default::default()
        };
        cfg.ensure_caching_sha2_rsa();
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
        let mut cfg = cfg;
        cfg.ensure_caching_sha2_rsa();
        let server = TestServer::start_with_handshake("auth_sha2", cfg).await;
        let mut client = server.connect_as("root", "secret").await;
        assert!(matches!(
            client.query("SELECT 1").await,
            rusql_protocol::client_decode::QueryResponse::Rows { .. }
        ));
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn accepts_caching_sha2_rsa_when_auth_enabled() {
        let mut cfg = HandshakeConfig {
            auth_credentials: Some(AuthCredentials {
                username: "root".into(),
                password: "secret".into(),
            }),
            ..Default::default()
        };
        cfg.ensure_caching_sha2_rsa();
        let server = TestServer::start_with_handshake("auth_rsa", cfg).await;
        let mut client = server.connect_rsa_as("root", "secret").await;
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
    use crate::test_support::{TestServer, WireClient};
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
    async fn update_across_connections() {
        let server = TestServer::start("update_conn").await;
        let mut c1 = server.connect().await;
        assert!(matches!(
            c1.query("CREATE TABLE u (id INT, name VARCHAR(8))").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            c1.query("INSERT INTO u VALUES (1, 'a')").await,
            QueryResponse::Ok { .. }
        ));
        c1.quit().await;

        let mut c2 = server.connect().await;
        assert!(matches!(
            c2.query("UPDATE u SET name = 'b' WHERE id = 1").await,
            QueryResponse::Ok { .. }
        ));
        match c2.query("SELECT name FROM u WHERE id = 1").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows[0][0], "b"),
            other => panic!("expected rows, got {other:?}"),
        }
        c2.quit().await;
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

        let eng = server.reopen_engine();
        assert_eq!(eng.scan("tx").unwrap(), vec![vec!["2".to_string()]]);

        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn transaction_rollback_leaves_wal_unchanged() {
        let server = TestServer::start("txn-wal").await;
        let wal_path = server.data_dir.join("rusql.wal");

        let mut c1 = server.connect().await;
        assert!(matches!(
            c1.query("CREATE TABLE tw (id INT)").await,
            QueryResponse::Ok { .. }
        ));
        let wal_after_ddl = std::fs::read_to_string(&wal_path).unwrap_or_default();

        assert!(matches!(c1.query("BEGIN").await, QueryResponse::Ok { .. }));
        assert!(matches!(
            c1.query("INSERT INTO tw VALUES (1)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            c1.query("ROLLBACK").await,
            QueryResponse::Ok { .. }
        ));
        c1.quit().await;

        let wal_after_rollback = std::fs::read_to_string(&wal_path).unwrap_or_default();
        assert_eq!(
            wal_after_ddl, wal_after_rollback,
            "ROLLBACK must not flush transaction overlay to WAL"
        );

        let eng = server.reopen_engine();
        assert_eq!(eng.scan("tw").unwrap(), Vec::<Vec<String>>::new());

        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// Issue #73 — official MySQL 8.0 CLI version probe must return a resultset.
    #[tokio::test]
    async fn mysql_cli_version_probe_returns_rows() {
        let server = TestServer::start("version_probe_rows").await;
        let mut client = server.connect_like_mysql_cli().await;
        match client.query("select @@version_comment limit 1").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["@@version_comment".to_string()]);
                assert_eq!(rows.len(), 1);
                assert!(!rows[0][0].is_empty());
            }
            other => panic!("version probe must return rows, got {other:?}"),
        }
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// Issue #73 — official MySQL 8.0 CLI sends CLIENT_QUERY_ATTRIBUTES on COM_QUERY.
    #[tokio::test]
    async fn mysql_cli_query_attributes_compat() {
        let server = TestServer::start("mysql_cli_attrs").await;

        let mut c1 = server.connect_like_mysql_cli().await;
        assert!(matches!(
            c1.query("CREATE TABLE cli73 (id INT, name VARCHAR(8))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            c1.query("INSERT INTO cli73 VALUES (1, 'a')").await,
            QueryResponse::Ok { .. }
        ));
        c1.quit().await;

        let mut c2 = server.connect_like_mysql_cli().await;
        match c2.query("SELECT * FROM cli73").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string(), "a".to_string()]]);
            }
            other => panic!("INSERT must persist across connections: {other:?}"),
        }
        assert!(matches!(
            c2.query("UPDATE cli73 SET name = 'b' WHERE id = 1").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            c2.query("DELETE FROM cli73 WHERE id = 1").await,
            QueryResponse::Ok { .. }
        ));
        match c2.query("SHOW TABLES").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.iter().any(|r| r[0] == "cli73")),
            other => panic!("SHOW TABLES failed: {other:?}"),
        }
        match c2.query("DESCRIBE cli73").await {
            QueryResponse::Rows { columns, .. } => assert_eq!(columns[0], "Field"),
            other => panic!("DESCRIBE failed: {other:?}"),
        }
        assert!(matches!(c2.query("BEGIN").await, QueryResponse::Ok { .. }));
        assert!(matches!(
            c2.query("INSERT INTO cli73 VALUES (2, 'z')").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(c2.query("COMMIT").await, QueryResponse::Ok { .. }));
        c2.quit().await;

        let eng = server.reopen_engine();
        assert_eq!(
            eng.scan("cli73").unwrap(),
            vec![vec!["2".to_string(), "z".to_string()]]
        );

        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn mysql_cli_with_version_probe_separate_connections() {
        let server = TestServer::start("version_probe").await;

        async fn version_probe(client: &mut WireClient) {
            let _ = client.query("select @@version_comment limit 1").await;
        }

        let mut c1 = server.connect_like_mysql_cli().await;
        version_probe(&mut c1).await;
        assert!(matches!(
            c1.query("CREATE TABLE md_t (id INT, name VARCHAR(32))")
                .await,
            QueryResponse::Ok { .. }
        ));
        c1.quit().await;

        let mut c2 = server.connect_like_mysql_cli().await;
        version_probe(&mut c2).await;
        assert!(matches!(
            c2.query("INSERT INTO md_t VALUES (1, 'alice')").await,
            QueryResponse::Ok { .. }
        ));
        c2.quit().await;

        let mut c3 = server.connect_like_mysql_cli().await;
        version_probe(&mut c3).await;
        match c3.query("SELECT * FROM md_t").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string(), "alice".to_string()]]);
            }
            other => panic!("expected row after version probe path, got {other:?}"),
        }
        c3.quit().await;

        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn mysql_cli_create_insert_on_separate_connections() {
        let server = TestServer::start("sep_conn_cli").await;

        let mut c1 = server.connect_like_mysql_cli().await;
        assert!(matches!(
            c1.query("CREATE TABLE md_t (id INT, name VARCHAR(32))")
                .await,
            QueryResponse::Ok { .. }
        ));
        c1.quit().await;

        let mut c2 = server.connect_like_mysql_cli().await;
        assert!(matches!(
            c2.query("INSERT INTO md_t VALUES (1, 'alice')").await,
            QueryResponse::Ok { .. }
        ));
        c2.quit().await;

        let mut c3 = server.connect_like_mysql_cli().await;
        match c3.query("SELECT * FROM md_t").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string(), "alice".to_string()]]);
            }
            other => panic!("expected row after cross-connection INSERT, got {other:?}"),
        }
        c3.quit().await;

        let eng = server.reopen_engine();
        assert_eq!(
            eng.scan("md_t").unwrap(),
            vec![vec!["1".to_string(), "alice".to_string()]]
        );

        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn mysql_cli_plain_com_query_when_attrs_negotiated() {
        let server = TestServer::start("plain_com_query").await;
        let mut client = server.connect_like_mysql_cli().await;
        match client.query_plain("SELECT 1").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("plain COM_QUERY must return rows when attrs negotiated: {other:?}"),
        }
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn mysql_cli_query_attributes_mysql_diff_fixture() {
        let server = TestServer::start("mysql_diff_fixture").await;

        let mut c1 = server.connect_like_mysql_cli().await;
        for sql in [
            "CREATE TABLE md_t (id INT NOT NULL, name VARCHAR(32), PRIMARY KEY (id))",
            "INSERT INTO md_t (id, name) VALUES (1, 'alice')",
            "INSERT INTO md_t (id, name) VALUES (2, 'bob')",
        ] {
            assert!(
                matches!(c1.query(sql).await, QueryResponse::Ok { .. }),
                "failed on: {sql}"
            );
        }
        c1.quit().await;

        let mut c2 = server.connect_like_mysql_cli().await;
        match c2.query("SELECT id, name FROM md_t ORDER BY id").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "alice".to_string()],
                        vec!["2".to_string(), "bob".to_string()],
                    ]
                );
            }
            other => panic!("expected two rows from mysql-diff fixture, got {other:?}"),
        }
        c2.quit().await;

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
    async fn stmt_prepare_execute_binary_table_select() {
        let server = TestServer::start("stmt_binary").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE TABLE bt (id INT, name VARCHAR(8))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO bt VALUES (7, 'seven')").await,
            QueryResponse::Ok { .. }
        ));

        let stmt_id = client.stmt_prepare("SELECT id, name FROM bt").await;
        match client.stmt_execute(stmt_id, &[]).await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["id".to_string(), "name".to_string()]);
                assert_eq!(rows, vec![vec!["7".to_string(), "seven".to_string()]]);
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

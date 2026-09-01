//! Per-connection command loop after handshake.

use crate::prepared::PreparedStatementStore;
use rusql_core::{
    parse_account_ddl, AccountDdl, ConnectionRegistry, PrivilegeStore, ProgramStore, Session,
    AUTH_PLUGIN_CACHING_SHA2,
};
use rusql_executor::{
    check_statement_privilege, execute, execute_grant, execute_revoke, execute_stored_program,
    ExecError, QueryResult,
};
use rusql_planner::plan;
use rusql_protocol::{
    authenticate_change_user, authenticate_handshake, binary_resultset_for_client, err_packet,
    exchange_handshake, field_list_response, mysql_type_from_sql_type, ok_packet_for_client,
    parse_command, parse_stmt_execute, read_packet, stmt_eof_packet_for_client,
    stmt_field_definition, stmt_prepare_ok, text_resultset_for_client, write_packets,
    AuthLookupResult, ChangeUserRequest, ClientCommand, HandshakeConfig, HandshakeSession,
    ProtocolError, MYSQL_TYPE_VAR_STRING,
};
use rusql_sql::{parse_for_session, try_parse_stored_program};
use rusql_storage::{
    read_binlog_file, BinlogWriter, OverlayEngine, PersistentEngine, ReadOnlyEngine,
    TransactionState,
};
use sqlparser::ast::Statement;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::RwLock as AsyncRwLock;
use tracing::{debug, warn};

async fn resolve_auth_lookup(
    privileges: &Arc<AsyncRwLock<PrivilegeStore>>,
    config: &HandshakeConfig,
    username: &str,
    client_host: &str,
) -> Result<Option<AuthLookupResult>, ProtocolError> {
    if config.auth_credentials.is_some() {
        return Ok(None);
    }
    let store = privileges.read().await;
    if !store.has_accounts() {
        return Ok(None);
    }
    if let Some(account) = store.resolve_auth(username, client_host) {
        return Ok(Some(AuthLookupResult {
            password: account.password,
            auth_plugin: account.auth_plugin,
            account_host: account.host,
        }));
    }
    if PrivilegeStore::is_superuser(username) {
        return Ok(Some(AuthLookupResult {
            password: String::new(),
            auth_plugin: AUTH_PLUGIN_CACHING_SHA2.to_string(),
            account_host: "%".into(),
        }));
    }
    Ok(None)
}

/// Run handshake then process COM_* commands until QUIT or disconnect.
#[allow(clippy::too_many_arguments)]
pub async fn serve_connection<S>(
    stream: &mut S,
    config: &HandshakeConfig,
    connection_id: u32,
    engine: Arc<tokio::sync::RwLock<PersistentEngine>>,
    privileges: Arc<AsyncRwLock<PrivilegeStore>>,
    registry: Arc<ConnectionRegistry>,
    data_dir: PathBuf,
    client_host: &str,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut conn_config = config.clone();
    if privileges.read().await.has_accounts() || conn_config.auth_credentials.is_some() {
        conn_config.ensure_caching_sha2_rsa();
    }
    let (handshake, response) = exchange_handshake(stream, &conn_config, connection_id).await?;
    let lookup =
        resolve_auth_lookup(&privileges, &conn_config, &response.username, client_host).await?;
    if conn_config.auth_credentials.is_none()
        && privileges.read().await.has_accounts()
        && lookup.is_none()
    {
        let err = err_packet(1045, &rusql_i18n::messages::protocol_access_denied());
        let _ = write_packets(stream, 2, &[err]).await;
        return Err(ProtocolError::Message(
            rusql_i18n::messages::protocol_access_denied(),
        ));
    }
    let hs = authenticate_handshake(stream, &conn_config, &handshake, &response, lookup).await?;
    let conn_id = hs.connection_id as u64;
    registry.register(
        conn_id,
        &hs.username,
        client_host,
        hs.database.as_deref().unwrap_or("rusql"),
    );
    let result = run_command_loop(
        stream,
        hs,
        conn_config,
        engine,
        privileges,
        registry.clone(),
        data_dir,
        client_host,
    )
    .await;
    registry.unregister(conn_id);
    result
}

#[allow(clippy::too_many_arguments)]
async fn run_command_loop<S>(
    stream: &mut S,
    mut hs: HandshakeSession,
    conn_config: HandshakeConfig,
    engine: Arc<tokio::sync::RwLock<PersistentEngine>>,
    privileges: Arc<AsyncRwLock<PrivilegeStore>>,
    registry: Arc<ConnectionRegistry>,
    data_dir: PathBuf,
    client_host: &str,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut session = Session::new(hs.connection_id as u64, hs.username.clone());
    session.host = hs.account_host.clone();
    if let Some(db) = hs.database.clone() {
        session.database = db;
    }
    session.process_list = Some(registry.clone());
    let programs = Arc::new(AsyncRwLock::new(
        ProgramStore::load(&data_dir).unwrap_or_default(),
    ));
    let binlog = Arc::new(AsyncRwLock::new(
        BinlogWriter::open(&data_dir, hs.connection_id)
            .map_err(|e| ProtocolError::Message(e.to_string()))?,
    ));
    {
        let store = programs.read().await;
        store.seed_catalog(&mut session.catalog);
    }
    seed_session_catalog(&mut session, &engine).await;
    let mut txn: Option<TransactionState> = None;
    let mut stmts = PreparedStatementStore::new();

    loop {
        registry.set_sleep(session.id);
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
                let _ = stream.shutdown().await;
                break;
            }
            ClientCommand::InitDb(db) => {
                debug!(connection_id = hs.connection_id, %db, "com_init_db");
                registry.set_command(session.id, "Init DB", Some(&db));
                if let Err(e) =
                    handle_init_db(stream, &mut session, &db, hs.client_capabilities, &engine).await
                {
                    warn!(connection_id = hs.connection_id, error = %e, "init db failed");
                }
                registry.update_session(session.id, &session.user, client_host, &session.database);
            }
            ClientCommand::Ping => {
                debug!(connection_id = hs.connection_id, "com_ping");
                registry.set_command(session.id, "Ping", None);
                let ok = ok_packet_for_client(0, 0, hs.client_capabilities);
                write_packets(stream, 1, &[ok]).await?;
            }
            ClientCommand::FieldList { table, .. } => {
                debug!(connection_id = hs.connection_id, %table, "com_field_list");
                registry.set_command(session.id, "FieldList", Some(&table));
                if let Err(e) =
                    handle_field_list(stream, &session, &table, hs.client_capabilities).await
                {
                    warn!(connection_id = hs.connection_id, error = %e, "field list failed");
                }
            }
            ClientCommand::ProcessInfo => {
                debug!(connection_id = hs.connection_id, "com_process_info");
                registry.set_command(session.id, "Query", Some("SHOW PROCESSLIST"));
                if let Err(e) =
                    handle_process_info(stream, &registry, hs.connection_id, hs.client_capabilities)
                        .await
                {
                    warn!(connection_id = hs.connection_id, error = %e, "process info failed");
                }
            }
            ClientCommand::ChangeUser(req) => {
                debug!(connection_id = hs.connection_id, user = %req.username, "com_change_user");
                registry.set_command(session.id, "Change user", Some(&req.username));
                let encoded = req.encode(hs.client_capabilities);
                match handle_change_user(
                    stream,
                    &conn_config,
                    &hs,
                    &encoded,
                    &privileges,
                    client_host,
                    &mut session,
                    &engine,
                )
                .await
                {
                    Ok(updated) => {
                        hs = updated;
                        registry.update_session(
                            session.id,
                            &session.user,
                            client_host,
                            &session.database,
                        );
                    }
                    Err(e) => {
                        warn!(connection_id = hs.connection_id, error = %e, "change user failed");
                    }
                }
            }
            ClientCommand::ResetConnection => {
                debug!(connection_id = hs.connection_id, "com_reset_connection");
                registry.set_command(session.id, "Reset connection", None);
                if let Err(e) =
                    handle_reset_connection(stream, &mut txn, &mut stmts, hs.client_capabilities)
                        .await
                {
                    warn!(
                        connection_id = hs.connection_id,
                        error = %e,
                        "reset connection failed"
                    );
                }
            }
            ClientCommand::Query(sql) => {
                debug!(connection_id = hs.connection_id, %sql, "com_query");
                registry.set_command(session.id, "Query", Some(&sql));
                if let Err(e) = execute_sql(
                    stream,
                    &mut session,
                    &engine,
                    &privileges,
                    &programs,
                    &binlog,
                    &data_dir,
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
            ClientCommand::BinlogDump { position, .. } => {
                debug!(
                    connection_id = hs.connection_id,
                    position, "com_binlog_dump"
                );
                registry.set_command(session.id, "Binlog Dump", None);
                if let Err(e) =
                    handle_binlog_dump(stream, &binlog, position, hs.client_capabilities).await
                {
                    warn!(connection_id = hs.connection_id, error = %e, "binlog dump failed");
                }
            }
            ClientCommand::RegisterSlave => {
                debug!(connection_id = hs.connection_id, "com_register_slave");
                let ok = ok_packet_for_client(0, 0, hs.client_capabilities);
                write_packets(stream, 1, &[ok]).await?;
            }
            ClientCommand::StmtPrepare(sql) => {
                debug!(connection_id = hs.connection_id, %sql, "com_stmt_prepare");
                registry.set_command(session.id, "Prepare", Some(&sql));
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
                registry.set_command(session.id, "Execute", None);
                if let Err(e) = handle_stmt_execute(
                    stream,
                    &mut session,
                    &engine,
                    &privileges,
                    &programs,
                    &binlog,
                    &data_dir,
                    &mut txn,
                    &mut stmts,
                    stmt_id,
                    &payload,
                    hs.client_capabilities,
                )
                .await
                {
                    warn!(connection_id = hs.connection_id, error = %e, "stmt execute failed");
                }
            }
            ClientCommand::StmtSendLongData {
                stmt_id,
                param_id,
                data,
            } => {
                if let Err(e) = stmts.append_long_data(stmt_id, param_id, &data) {
                    let err = err_packet(1210, &e);
                    write_packets(stream, 1, &[err]).await?;
                }
            }
            ClientCommand::StmtReset { stmt_id } => {
                if stmts.reset(stmt_id) {
                    let ok = ok_packet_for_client(0, 0, hs.client_capabilities);
                    write_packets(stream, 1, &[ok]).await?;
                } else {
                    let err = err_packet(1210, "unknown prepared statement handler");
                    write_packets(stream, 1, &[err]).await?;
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

async fn seed_session_catalog(session: &mut Session, engine: &Arc<AsyncRwLock<PersistentEngine>>) {
    let eng = engine.read().await;
    for meta in eng.table_metas() {
        session.catalog.create_table(meta);
    }
}

async fn handle_field_list<S>(
    stream: &mut S,
    session: &Session,
    table: &str,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Some(meta) = session.catalog.get_table(table) else {
        let err = err_packet(1146, &format!("Table '{table}' doesn't exist"));
        write_packets(stream, 1, &[err]).await?;
        return Ok(());
    };
    let columns: Vec<(String, u8)> = meta
        .columns
        .iter()
        .map(|c| (c.name.clone(), mysql_type_from_sql_type(&c.data_type)))
        .collect();
    let packets = field_list_response(&columns, client_caps);
    write_packets(stream, 1, &packets).await?;
    Ok(())
}

async fn handle_process_info<S>(
    stream: &mut S,
    registry: &ConnectionRegistry,
    connection_id: u32,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let row = registry.current(connection_id as u64);
    let columns = vec![
        "Id".into(),
        "User".into(),
        "Host".into(),
        "db".into(),
        "Command".into(),
        "Time".into(),
        "State".into(),
        "Info".into(),
    ];
    let rows = if let Some(r) = row {
        vec![vec![
            r.id.to_string(),
            r.user,
            r.host,
            r.db,
            r.command,
            r.time.to_string(),
            r.state,
            r.info.unwrap_or_default(),
        ]]
    } else {
        vec![]
    };
    let packets = text_resultset_for_client(&columns, &rows, client_caps);
    write_packets(stream, 1, &packets).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_change_user<S>(
    stream: &mut S,
    config: &HandshakeConfig,
    hs: &HandshakeSession,
    payload: &[u8],
    privileges: &Arc<AsyncRwLock<PrivilegeStore>>,
    client_host: &str,
    session: &mut Session,
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
) -> Result<HandshakeSession, ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let request = ChangeUserRequest::decode(payload, hs.client_capabilities)?;
    let lookup = resolve_auth_lookup(privileges, config, &request.username, client_host).await?;
    if config.auth_credentials.is_none()
        && privileges.read().await.has_accounts()
        && lookup.is_none()
        && !PrivilegeStore::is_superuser(&request.username)
    {
        let err = err_packet(1045, &rusql_i18n::messages::protocol_access_denied());
        write_packets(stream, 1, &[err]).await?;
        return Err(ProtocolError::Message(
            rusql_i18n::messages::protocol_access_denied(),
        ));
    }
    let updated = authenticate_change_user(stream, config, hs, payload, lookup).await?;
    session.user = updated.username.clone();
    session.host = updated.account_host.clone();
    if let Some(ref db) = updated.database {
        session.database = db.clone();
        seed_session_catalog(session, engine).await;
    }
    Ok(updated)
}

async fn handle_reset_connection<S>(
    stream: &mut S,
    txn: &mut Option<TransactionState>,
    stmts: &mut PreparedStatementStore,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    *txn = None;
    *stmts = PreparedStatementStore::new();
    let ok = ok_packet_for_client(0, 0, client_caps);
    write_packets(stream, 1, &[ok]).await?;
    Ok(())
}

fn is_kill_query(sql: &str) -> bool {
    sql.trim()
        .trim_end_matches(';')
        .to_ascii_uppercase()
        .starts_with("KILL")
}

async fn handle_init_db<S>(
    stream: &mut S,
    session: &mut Session,
    database: &str,
    client_caps: u32,
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let known = {
        let eng = engine.read().await;
        eng.list_databases()
    };
    if !known.iter().any(|d| d == database) {
        let err = err_packet(1049, &format!("Unknown database '{database}'"));
        write_packets(stream, 1, &[err]).await?;
        return Ok(());
    }
    session.database = database.to_string();
    let ok = ok_packet_for_client(0, 0, client_caps);
    write_packets(stream, 1, &[ok]).await?;
    Ok(())
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
async fn handle_binlog_dump<S>(
    stream: &mut S,
    binlog: &Arc<AsyncRwLock<BinlogWriter>>,
    position: u32,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let writer = binlog.read().await;
    let path = writer.current_path().to_path_buf();
    drop(writer);
    let data = read_binlog_file(&path).map_err(|e| ProtocolError::Message(e.to_string()))?;
    let start = position.min(data.len() as u32) as usize;
    if start < data.len() {
        // Replication event packet: 0-byte header + raw event bytes (MVP).
        let chunk = &data[start..];
        write_packets(stream, 1, &[chunk.to_vec()]).await?;
    }
    let ok = ok_packet_for_client(0, 0, client_caps);
    write_packets(stream, 2, &[ok]).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_stmt_execute<S>(
    stream: &mut S,
    session: &mut Session,
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
    privileges: &Arc<AsyncRwLock<PrivilegeStore>>,
    programs: &Arc<AsyncRwLock<ProgramStore>>,
    binlog: &Arc<AsyncRwLock<BinlogWriter>>,
    data_dir: &Path,
    txn: &mut Option<TransactionState>,
    store: &mut PreparedStatementStore,
    stmt_id: u32,
    payload: &[u8],
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Some((param_count, result_columns, result_column_types)) = store.get(stmt_id).map(|stmt| {
        (
            stmt.param_count,
            stmt.result_columns.clone(),
            stmt.result_column_types.clone(),
        )
    }) else {
        let err = err_packet(1210, "unknown prepared statement handler");
        write_packets(stream, 1, &[err]).await?;
        return Ok(());
    };
    let params = match parse_stmt_execute(payload, param_count) {
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
    store.take_long_data(stmt_id);
    let binary_types = if result_columns.is_empty() {
        None
    } else {
        Some(result_column_types.as_slice())
    };
    execute_sql(
        stream,
        session,
        engine,
        privileges,
        programs,
        binlog,
        data_dir,
        txn,
        &sql,
        1,
        binary_types,
        client_caps,
    )
    .await
}

fn is_read_only_statement(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::Query(_)
            | Statement::Explain { .. }
            | Statement::ExplainTable { .. }
            | Statement::ShowColumns { .. }
            | Statement::ShowCreate { .. }
            | Statement::ShowTables { .. }
            | Statement::ShowDatabases { .. }
            | Statement::Use(_)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplicationStatusKind {
    Master,
    Slave,
}

fn try_parse_replication_status(sql: &str) -> Option<ReplicationStatusKind> {
    let upper = sql.trim().trim_end_matches(';').to_ascii_uppercase();
    if upper == "SHOW MASTER STATUS" {
        return Some(ReplicationStatusKind::Master);
    }
    if upper == "SHOW SLAVE STATUS" || upper == "SHOW REPLICA STATUS" {
        return Some(ReplicationStatusKind::Slave);
    }
    None
}

async fn write_replication_status<S>(
    stream: &mut S,
    kind: ReplicationStatusKind,
    binlog: &Arc<AsyncRwLock<BinlogWriter>>,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let writer = binlog.read().await;
    let gtid = writer.gtid_state().clone();
    let file = writer
        .current_path()
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("binlog.000001")
        .to_string();
    let pos = format!("{}", gtid.sequence.saturating_mul(100));
    drop(writer);
    let (columns, rows) = match kind {
        ReplicationStatusKind::Master => (
            vec![
                "File".into(),
                "Position".into(),
                "Binlog_Do_DB".into(),
                "Binlog_Ignore_DB".into(),
                "Executed_Gtid_Set".into(),
            ],
            vec![vec![
                file,
                pos,
                String::new(),
                String::new(),
                format!("{}:{}", gtid.server_uuid, gtid.sequence),
            ]],
        ),
        ReplicationStatusKind::Slave => (
            vec![
                "Slave_IO_Running".into(),
                "Slave_SQL_Running".into(),
                "Retrieved_Gtid_Set".into(),
                "Executed_Gtid_Set".into(),
            ],
            vec![vec![
                "No".into(),
                "No".into(),
                String::new(),
                gtid.applied.join(","),
            ]],
        ),
    };
    let payloads = text_resultset_for_client(&columns, &rows, client_caps);
    write_packets(stream, 1, &payloads).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn execute_sql<S>(
    stream: &mut S,
    session: &mut Session,
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
    privileges: &Arc<AsyncRwLock<PrivilegeStore>>,
    programs: &Arc<AsyncRwLock<ProgramStore>>,
    binlog: &Arc<AsyncRwLock<BinlogWriter>>,
    data_dir: &Path,
    txn: &mut Option<TransactionState>,
    sql: &str,
    seq_start: u8,
    binary_column_types: Option<&[u8]>,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if let Some(ddl) = parse_account_ddl(sql) {
        return handle_account_ddl(stream, session, privileges, data_dir, ddl, client_caps).await;
    }

    if is_kill_query(sql) {
        let err = err_packet(
            1295,
            "KILL is not supported in this milestone (documented stub for M53)",
        );
        write_packets(stream, 1, &[err]).await?;
        return Ok(());
    }

    if let Some(status) = try_parse_replication_status(sql) {
        return write_replication_status(stream, status, binlog, client_caps).await;
    }

    if let Some(stmt) = try_parse_stored_program(sql) {
        let result = {
            let store = privileges.read().await;
            let mut prog_store = programs.write().await;
            let mut eng = engine.write().await;
            match txn {
                Some(ref mut t) => {
                    let mut overlay = OverlayEngine::new(&eng, t);
                    execute_stored_program(
                        &mut overlay,
                        session,
                        &mut prog_store,
                        stmt,
                        Some(&store),
                    )
                }
                None => {
                    execute_stored_program(&mut *eng, session, &mut prog_store, stmt, Some(&store))
                }
            }
        };
        match result {
            Ok(r) => {
                if let Err(e) = programs.read().await.save(data_dir) {
                    let err = err_packet(1105, &e.to_string());
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                let ok = ok_packet_for_client(
                    match r {
                        QueryResult::Ok { rows_affected } => rows_affected,
                        _ => 0,
                    },
                    0,
                    client_caps,
                );
                write_packets(stream, seq_start, &[ok]).await?;
            }
            Err(e) => write_exec_error(stream, e).await?,
        }
        return Ok(());
    }

    let statements = match parse_for_session(sql, &session.user, &session.host) {
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
                let records: Vec<_> = state.pending_records().to_vec();
                let mut eng = engine.write().await;
                if let Err(e) = eng.commit_transaction(state.pending_records()) {
                    let err = err_packet(1105, &e.to_string());
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                drop(eng);
                {
                    let mut writer = binlog.write().await;
                    if let Err(e) = writer.append_commit(data_dir, &session.database, &records) {
                        let err = err_packet(1105, &e.to_string());
                        write_packets(stream, 1, &[err]).await?;
                        return Ok(());
                    }
                }
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
            Statement::Grant {
                privileges: grant_privileges,
                objects,
                grantees,
                with_grant_option,
                ..
            } => {
                let result = {
                    let mut store = privileges.write().await;
                    let result = execute_grant(
                        &mut store,
                        session,
                        &grant_privileges,
                        &objects,
                        &grantees,
                        with_grant_option,
                    );
                    if result.is_ok() {
                        store.save(data_dir).map_err(|e| {
                            ProtocolError::Message(format!("failed to save privileges: {e}"))
                        })?;
                    }
                    result
                };
                match result {
                    Ok(r) => all_results.push(r),
                    Err(e) => {
                        write_exec_error(stream, e).await?;
                        return Ok(());
                    }
                }
            }
            Statement::Revoke {
                privileges: revoke_privileges,
                objects,
                grantees,
                ..
            } => {
                let result = {
                    let mut store = privileges.write().await;
                    let result = execute_revoke(
                        &mut store,
                        session,
                        &revoke_privileges,
                        &objects,
                        &grantees,
                    );
                    if result.is_ok() {
                        store.save(data_dir).map_err(|e| {
                            ProtocolError::Message(format!("failed to save privileges: {e}"))
                        })?;
                    }
                    result
                };
                match result {
                    Ok(r) => all_results.push(r),
                    Err(e) => {
                        write_exec_error(stream, e).await?;
                        return Ok(());
                    }
                }
            }
            other => {
                if let Err(e) = {
                    let store = privileges.read().await;
                    check_statement_privilege(&store, session, &other)
                } {
                    write_exec_error(stream, e).await?;
                    return Ok(());
                }
                let read_only = is_read_only_statement(&other);
                let plans = plan(session, vec![other]);
                let results = {
                    let store = privileges.read().await;
                    if read_only {
                        let eng = engine.read().await;
                        match txn {
                            Some(ref mut t) => {
                                let mut overlay = OverlayEngine::new(&eng, t);
                                execute(&mut overlay, session, &plans, Some(&store))
                            }
                            None => {
                                let mut view = ReadOnlyEngine::new(&eng);
                                execute(&mut view, session, &plans, Some(&store))
                            }
                        }
                    } else {
                        let mut eng = engine.write().await;
                        match txn {
                            Some(ref mut t) => {
                                let mut overlay = OverlayEngine::new(&eng, t);
                                execute(&mut overlay, session, &plans, Some(&store))
                            }
                            None => execute(&mut *eng, session, &plans, Some(&store)),
                        }
                    }
                };
                match results {
                    Ok(r) => all_results.extend(r),
                    Err(e) => {
                        write_exec_error(stream, e).await?;
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

async fn handle_account_ddl<S>(
    stream: &mut S,
    session: &Session,
    privileges: &Arc<AsyncRwLock<PrivilegeStore>>,
    data_dir: &Path,
    ddl: AccountDdl,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if !PrivilegeStore::is_superuser(&session.user) {
        let err = err_packet(
            1227,
            &rusql_i18n::messages::account_admin_required(&session.user, &session.host),
        );
        write_packets(stream, 1, &[err]).await?;
        return Ok(());
    }

    let mut store = privileges.write().await;
    let exec_result = match ddl {
        AccountDdl::CreateUser {
            accounts,
            auth_plugin,
            password,
            if_not_exists,
        } => {
            for account in &accounts {
                if let Err(message) = store.create_user(
                    account,
                    password.clone(),
                    auth_plugin.clone(),
                    if_not_exists,
                ) {
                    return write_exec_error(stream, ExecError::Message(message)).await;
                }
            }
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        AccountDdl::DropUser {
            accounts,
            if_exists,
        } => {
            for account in &accounts {
                if let Err(message) = store.drop_user(account, if_exists) {
                    return write_exec_error(stream, ExecError::Message(message)).await;
                }
            }
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
    };
    if exec_result.is_ok() {
        store
            .save(data_dir)
            .map_err(|e| ProtocolError::Message(format!("failed to save privileges: {e}")))?;
    }

    match exec_result {
        Ok(QueryResult::Ok { .. }) => {
            let ok = ok_packet_for_client(0, 0, client_caps);
            write_packets(stream, 1, &[ok]).await?;
        }
        Ok(_) => {}
        Err(e) => write_exec_error(stream, e).await?,
    }
    Ok(())
}

async fn write_exec_error<S>(stream: &mut S, error: ExecError) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let err = match error {
        ExecError::Message(m) => err_packet(1105, &m),
        ExecError::Mysql { code, message } => err_packet(code, &message),
        ExecError::Storage(e) => err_packet(1146, &e.to_string()),
    };
    write_packets(stream, 1, &[err]).await?;
    Ok(())
}

#[cfg(test)]
mod auth_tests {
    use crate::test_support::TestServer;
    use rusql_core::PrivilegeStore;
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

    #[tokio::test]
    async fn create_user_login_and_drop_native_password() {
        let server = TestServer::start("multi_user_native").await;
        let mut admin = server.connect().await;
        assert!(matches!(
            admin
                .query("CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'")
                .await,
            rusql_protocol::client_decode::QueryResponse::Ok { .. }
        ));
        admin.quit().await;

        assert_eq!(
            server.try_connect_as("app", "wrong").await.unwrap_err(),
            0xFF
        );

        let mut app = server.connect_native_as("app", "secret").await;
        assert!(matches!(
            app.ping().await,
            rusql_protocol::client_decode::QueryResponse::Ok { .. }
        ));
        app.quit().await;

        let mut admin = server.connect().await;
        assert!(matches!(
            admin.query("DROP USER 'app'@'%'").await,
            rusql_protocol::client_decode::QueryResponse::Ok { .. }
        ));
        admin.quit().await;
        let store = PrivilegeStore::load(&server.data_dir).unwrap();
        assert!(store.resolve_auth("app", "127.0.0.1").is_none());
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{TestServer, WireClient};
    use rusql_protocol::client_decode::QueryResponse;
    use rusql_storage::{PersistentEngine, StorageEngine};

    /// Official `mysql`/`mysqladmin` oracle gates.
    ///
    /// On CI `ubuntu-latest` runners, `mysql` may be present but can hang forever
    /// against the embedded test server (observed in the rust job). Opt in with
    /// `RUSQL_ORACLE_MYSQL=1`. Locally (no `CI`), keep PATH-based discovery.
    fn oracle_mysql_cli_enabled() -> bool {
        if std::env::var_os("CI").is_some() && std::env::var_os("RUSQL_ORACLE_MYSQL").is_none() {
            return false;
        }
        std::process::Command::new("mysql")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn oracle_mysqladmin_enabled() -> bool {
        if std::env::var_os("CI").is_some() && std::env::var_os("RUSQL_ORACLE_MYSQL").is_none() {
            return false;
        }
        std::process::Command::new("mysqladmin")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn com_ping_ok() {
        let server = TestServer::start("com_ping").await;
        let mut client = server.connect().await;
        assert!(matches!(client.ping().await, QueryResponse::Ok { .. }));
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn com_init_db_ok_and_unknown() {
        let server = TestServer::start("com_init_db").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client.init_db("rusql").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.init_db("no_such_db").await,
            QueryResponse::Err { code: 1049, .. }
        ));
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

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
    async fn snapshot_isolation_two_connections() {
        let server = TestServer::start("snapshot_iso").await;
        let mut writer = server.connect().await;
        assert!(matches!(
            writer
                .query("CREATE TABLE snap (id INT, v VARCHAR(8))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            writer.query("INSERT INTO snap VALUES (1, 'a')").await,
            QueryResponse::Ok { .. }
        ));
        writer.quit().await;

        let mut reader = server.connect().await;
        assert!(matches!(
            reader.query("BEGIN").await,
            QueryResponse::Ok { .. }
        ));
        match reader.query("SELECT v FROM snap WHERE id = 1").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["a".to_string()]]);
            }
            other => panic!("expected snapshot row, got {other:?}"),
        }

        let mut writer = server.connect().await;
        assert!(matches!(
            writer.query("UPDATE snap SET v = 'b' WHERE id = 1").await,
            QueryResponse::Ok { .. }
        ));
        writer.quit().await;

        match reader.query("SELECT v FROM snap WHERE id = 1").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["a".to_string()]],
                    "reader txn must keep pinned snapshot"
                );
            }
            other => panic!("expected pinned snapshot row, got {other:?}"),
        }
        assert!(matches!(
            reader.query("COMMIT").await,
            QueryResponse::Ok { .. }
        ));
        reader.quit().await;

        let mut after = server.connect().await;
        match after.query("SELECT v FROM snap WHERE id = 1").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["b".to_string()]]);
            }
            other => panic!("expected committed update, got {other:?}"),
        }
        after.quit().await;
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

    /// Oracle gate: skipped unless `mysql` is on PATH (and on CI, `RUSQL_ORACLE_MYSQL=1`).
    /// Protocol coverage on CI remains via `mysql-diff` / smoke jobs.
    #[tokio::test]
    async fn official_mysql_client_select_1() {
        if !oracle_mysql_cli_enabled() {
            return;
        }

        let server = TestServer::start("official_mysql_cli").await;
        let port = server.addr.port().to_string();
        let output = std::process::Command::new("mysql")
            .args([
                "-h",
                "127.0.0.1",
                "-P",
                &port,
                "-u",
                "root",
                "--protocol=TCP",
                "--ssl-mode=DISABLED",
                "--connect-timeout=5",
                "-B",
                "-e",
                "SELECT 1",
            ])
            .output()
            .expect("spawn mysql");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "official mysql failed: status={:?} stderr={stderr}",
            output.status
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains('1'), "stdout={stdout}");

        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// Oracle gate: COM_INIT_DB via official `mysql` CLI (`USE` sends COM_INIT_DB).
    #[tokio::test]
    async fn official_mysql_client_use_rusql() {
        if !oracle_mysql_cli_enabled() {
            return;
        }

        let server = TestServer::start("official_mysql_use").await;
        let port = server.addr.port().to_string();
        let output = std::process::Command::new("mysql")
            .args([
                "-h",
                "127.0.0.1",
                "-P",
                &port,
                "-u",
                "root",
                "--protocol=TCP",
                "--ssl-mode=DISABLED",
                "--connect-timeout=5",
                "-B",
                "-e",
                "USE rusql",
            ])
            .output()
            .expect("spawn mysql");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "official mysql USE failed: status={:?} stderr={stderr}",
            output.status
        );

        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// Oracle gate: `mysqladmin ping` sends COM_PING (0x0E).
    #[tokio::test]
    async fn official_mysqladmin_ping() {
        if !oracle_mysqladmin_enabled() {
            return;
        }

        let server = TestServer::start("mysqladmin_ping").await;
        let port = server.addr.port().to_string();
        let output = std::process::Command::new("mysqladmin")
            .args([
                "-h",
                "127.0.0.1",
                "-P",
                &port,
                "-u",
                "root",
                "--protocol=TCP",
                "--ssl-mode=DISABLED",
                "--connect-timeout=5",
                "ping",
            ])
            .output()
            .expect("spawn mysqladmin");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "mysqladmin ping failed: status={:?} stderr={stderr}",
            output.status
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("alive"),
            "expected alive in stdout, got {stdout}"
        );

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

    #[tokio::test]
    async fn com_change_user_switches_database() {
        let server = TestServer::start("com_change_user").await;
        let mut client = server.connect().await;
        client.change_user("root", "", "rusql").await;
        assert!(matches!(
            client.query("SELECT 1").await,
            QueryResponse::Rows { .. }
        ));
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn show_processlist_lists_connection() {
        let server = TestServer::start("show_processlist").await;
        let mut client = server.connect().await;
        match client.query("SHOW PROCESSLIST").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Id");
                assert!(rows.iter().any(|r| r[4] == "Query"));
            }
            other => panic!("expected processlist rows, got {other:?}"),
        }
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn com_process_info_returns_current_row() {
        let server = TestServer::start("com_process_info").await;
        let mut client = server.connect().await;
        match client.process_info().await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns.len(), 8);
                assert_eq!(rows.len(), 1);
            }
            other => panic!("expected process info row, got {other:?}"),
        }
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn com_reset_connection_clears_prepared_statements() {
        let server = TestServer::start("com_reset").await;
        let mut client = server.connect().await;
        let stmt_id = client.stmt_prepare("SELECT 1").await;
        assert!(matches!(
            client.reset_connection().await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.stmt_reset(stmt_id).await,
            QueryResponse::Err { code: 1210, .. }
        ));
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn com_field_list_returns_columns() {
        let server = TestServer::start("com_field_list").await;
        let mut client = server.connect().await;
        assert!(matches!(
            client
                .query("CREATE TABLE fl (id INT, name VARCHAR(8))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert_eq!(client.field_list("fl").await, 2);
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn stmt_long_data_and_reset() {
        let server = TestServer::start("stmt_long_data").await;
        let mut client = server.connect().await;
        assert!(matches!(
            client.query("CREATE TABLE ld (msg VARCHAR(32))").await,
            QueryResponse::Ok { .. }
        ));
        let stmt_id = client.stmt_prepare("INSERT INTO ld VALUES (?)").await;
        client.stmt_send_long_data(stmt_id, 0, b"hel").await;
        client.stmt_send_long_data(stmt_id, 0, b"lo").await;
        assert!(matches!(
            client.stmt_execute(stmt_id, &[None]).await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT msg FROM ld").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows[0][0], "hello"),
            other => panic!("expected row, got {other:?}"),
        }
        assert!(matches!(
            client.stmt_reset(stmt_id).await,
            QueryResponse::Ok { .. }
        ));
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }
}

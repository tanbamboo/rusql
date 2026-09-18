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
    ProtocolError, BINLOG_DUMP_NON_BLOCK, COM_QUIT, MYSQL_TYPE_VAR_STRING,
};
use rusql_sql::{parse_for_session, try_parse_stored_program};
use rusql_storage::{
    dump_events_with_next_position, read_binlog_file, BinlogWriter, OverlayEngine,
    PersistentEngine, ReadOnlyEngine, TransactionState,
};
use sqlparser::ast::Statement;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::{watch, RwLock as AsyncRwLock};
use tracing::{debug, warn};

struct BinlogHub {
    writer: Arc<AsyncRwLock<BinlogWriter>>,
    commits: watch::Sender<u64>,
}

fn shared_binlog(data_dir: &Path) -> Result<Arc<BinlogHub>, ProtocolError> {
    static HUBS: OnceLock<Mutex<HashMap<PathBuf, Weak<BinlogHub>>>> = OnceLock::new();
    let mut hubs = HUBS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let key = data_dir.to_path_buf();
    if let Some(existing) = hubs.get(&key).and_then(Weak::upgrade) {
        return Ok(existing);
    }
    let writer =
        BinlogWriter::open(data_dir, 1).map_err(|e| ProtocolError::Message(e.to_string()))?;
    let (commits, _) = watch::channel(0u64);
    let hub = Arc::new(BinlogHub {
        writer: Arc::new(AsyncRwLock::new(writer)),
        commits,
    });
    hubs.insert(key, Arc::downgrade(&hub));
    Ok(hub)
}

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
    let hub = shared_binlog(&data_dir)?;
    let binlog = hub.writer.clone();
    let binlog_commits = hub.commits.clone();
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
                if let Err(e) = handle_reset_connection(
                    stream,
                    &mut session,
                    &mut txn,
                    &mut stmts,
                    hs.client_capabilities,
                )
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
                    &binlog_commits,
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
            ClientCommand::BinlogDump {
                position, flags, ..
            } => {
                debug!(
                    connection_id = hs.connection_id,
                    position, flags, "com_binlog_dump"
                );
                registry.set_command(session.id, "Binlog Dump", None);
                match handle_binlog_dump(
                    stream,
                    &binlog,
                    &binlog_commits,
                    position,
                    flags,
                    hs.client_capabilities,
                )
                .await
                {
                    Ok(true) => break,
                    Ok(false) => {}
                    Err(e) => {
                        warn!(connection_id = hs.connection_id, error = %e, "binlog dump failed");
                    }
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
                    &binlog_commits,
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
    session.last_insert_id = 0;
    session.row_count = -1;
    session.found_rows = 0;
    session.sql_calc_found_rows = false;
    session.clear_session_vars();
    if let Some(ref db) = updated.database {
        session.database = db.clone();
        seed_session_catalog(session, engine).await;
    }
    Ok(updated)
}

async fn handle_reset_connection<S>(
    stream: &mut S,
    session: &mut Session,
    txn: &mut Option<TransactionState>,
    stmts: &mut PreparedStatementStore,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    *txn = None;
    *stmts = PreparedStatementStore::new();
    session.last_insert_id = 0;
    session.row_count = -1;
    session.found_rows = 0;
    session.sql_calc_found_rows = false;
    session.clear_session_vars();
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

/// Returns `true` when the dump connection should disconnect (follow ended).
#[allow(clippy::too_many_arguments)]
async fn handle_binlog_dump<S>(
    stream: &mut S,
    binlog: &Arc<AsyncRwLock<BinlogWriter>>,
    binlog_commits: &watch::Sender<u64>,
    mut position: u32,
    flags: u16,
    client_caps: u32,
) -> Result<bool, ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let non_block = flags & BINLOG_DUMP_NON_BLOCK != 0;
    let mut rx = binlog_commits.subscribe();
    rx.borrow_and_update();
    let mut seq = 1u8;
    loop {
        let path = {
            let writer = binlog.read().await;
            writer.current_path().to_path_buf()
        };
        let data = read_binlog_file(&path).map_err(|e| ProtocolError::Message(e.to_string()))?;
        let (payloads, next) = dump_events_with_next_position(&data, position);
        if !payloads.is_empty() {
            if write_packets(stream, seq, &payloads).await.is_err() {
                return Ok(true);
            }
            seq = seq.wrapping_add(payloads.len() as u8);
            position = next;
        }
        if non_block {
            let ok = ok_packet_for_client(0, 0, client_caps);
            write_packets(stream, seq, &[ok]).await?;
            return Ok(false);
        }
        tokio::select! {
            changed = rx.changed() => {
                if changed.is_err() {
                    return Ok(true);
                }
            }
            pkt = read_packet(stream) => {
                match pkt {
                    Ok((_, payload)) if payload.first() == Some(&COM_QUIT) => {
                        let _ = stream.shutdown().await;
                        return Ok(true);
                    }
                    Ok(_) => {}
                    Err(_) => return Ok(true),
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_stmt_execute<S>(
    stream: &mut S,
    session: &mut Session,
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
    privileges: &Arc<AsyncRwLock<PrivilegeStore>>,
    programs: &Arc<AsyncRwLock<ProgramStore>>,
    binlog: &Arc<AsyncRwLock<BinlogWriter>>,
    binlog_commits: &watch::Sender<u64>,
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
        binlog_commits,
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
            | Statement::ShowVariables { .. }
            | Statement::ShowStatus { .. }
            | Statement::SetVariable { .. }
            | Statement::SetNames { .. }
            | Statement::SetNamesDefault {}
            | Statement::SetTransaction { .. }
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
    binlog_commits: &watch::Sender<u64>,
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
                    session.last_insert_id,
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
                if !records.is_empty() {
                    binlog_commits.send_modify(|n| *n = n.wrapping_add(1));
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
                let ok = ok_packet_for_client(rows_affected, session.last_insert_id, client_caps);
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
    use crate::test_support::{temp_data_dir, TestServer, WireClient};
    use rusql_protocol::client_decode::QueryResponse;
    use rusql_storage::{
        apply_binlog_file, extract_query_events, PersistentEngine, StorageEngine, BINLOG_MAGIC,
        EVENT_TYPE_DELETE_ROWS_V1, EVENT_TYPE_TABLE_MAP, EVENT_TYPE_UPDATE_ROWS_V1,
        EVENT_TYPE_WRITE_ROWS_V1,
    };

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

    /// Run blocking `mysql` CLI without stalling the embedded test server runtime.
    async fn mysql_cli_output(port: &str, extra: &[&str]) -> std::process::Output {
        let port = port.to_string();
        let extra: Vec<String> = extra.iter().map(|s| (*s).to_string()).collect();
        tokio::task::spawn_blocking(move || {
            let mut cmd = std::process::Command::new("mysql");
            cmd.args([
                "-h",
                "127.0.0.1",
                "-P",
                &port,
                "-u",
                "root",
                "--protocol=TCP",
                "--ssl-mode=DISABLED",
                "--connect-timeout=5",
            ]);
            cmd.args(extra);
            cmd.output().expect("spawn mysql")
        })
        .await
        .expect("mysql cli task")
    }

    async fn mysqladmin_output(port: &str, extra: &[&str]) -> std::process::Output {
        let port = port.to_string();
        let extra: Vec<String> = extra.iter().map(|s| (*s).to_string()).collect();
        tokio::task::spawn_blocking(move || {
            let mut cmd = std::process::Command::new("mysqladmin");
            cmd.args([
                "-h",
                "127.0.0.1",
                "-P",
                &port,
                "-u",
                "root",
                "--protocol=TCP",
                "--ssl-mode=DISABLED",
                "--connect-timeout=5",
            ]);
            cmd.args(extra);
            cmd.output().expect("spawn mysqladmin")
        })
        .await
        .expect("mysqladmin task")
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

    /// In-process server + official mysql CLI (isolates spawn vs protocol).
    #[tokio::test]
    async fn official_mysql_create_database_testserver() {
        if !oracle_mysql_cli_enabled() {
            return;
        }

        let server = TestServer::start("official_mysql_create_db").await;
        let port = server.addr.port().to_string();
        let create = mysql_cli_output(&port, &["-B", "-e", "CREATE DATABASE app_db"]).await;
        let create_stderr = String::from_utf8_lossy(&create.stderr);
        assert!(
            create.status.success(),
            "CREATE DATABASE failed: status={:?} stderr={create_stderr}",
            create.status
        );

        let use_db = mysql_cli_output(&port, &["-B", "-D", "app_db", "-e", "SELECT 1"]).await;
        let use_stderr = String::from_utf8_lossy(&use_db.stderr);
        assert!(
            use_db.status.success(),
            "handshake default database app_db failed: status={:?} stderr={use_stderr}",
            use_db.status
        );

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
        let output = mysql_cli_output(&port, &["-B", "-e", "SELECT 1"]).await;

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

    fn release_server_binary() -> Option<std::path::PathBuf> {
        let name = if cfg!(windows) {
            "rusql-server.exe"
        } else {
            "rusql-server"
        };
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/release")
            .join(name);
        path.exists().then_some(path)
    }

    /// Mirrors mysql-diff: release `rusql-server` subprocess must survive CREATE DATABASE + USE.
    #[tokio::test]
    async fn release_binary_multi_schema_sequence() {
        let Some(bin) = release_server_binary() else {
            eprintln!(
                "skip: build release rusql-server first (`cargo build --release -p rusql-server`)"
            );
            return;
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let data_dir = crate::test_support::temp_data_dir("release_multi_schema");
        let _ = std::fs::remove_dir_all(&data_dir);

        let mut child = std::process::Command::new(&bin)
            .args([
                "--port",
                &port.to_string(),
                "--data-dir",
                data_dir.to_str().unwrap(),
                "--wal-sync",
                "none",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn release rusql-server");

        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        for _ in 0..120 {
            if tokio::net::TcpStream::connect(addr).await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let mut c1 = crate::test_support::WireClient::connect_addr(addr).await;
        assert!(
            matches!(
                c1.query("CREATE DATABASE app_db").await,
                QueryResponse::Ok { .. }
            ),
            "CREATE DATABASE must succeed on release binary"
        );
        drop(c1);

        assert_eq!(
            child.try_wait().unwrap(),
            None,
            "release rusql-server must stay alive after CREATE DATABASE"
        );

        let mut c2 = crate::test_support::WireClient::connect_addr(addr).await;
        assert!(
            matches!(c2.init_db("app_db").await, QueryResponse::Ok { .. }),
            "COM_INIT_DB must succeed after CREATE DATABASE on release binary"
        );

        let _ = child.kill();
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// MySQL 8.0 CLI negotiates query attributes; release binary must survive CREATE DATABASE.
    #[tokio::test]
    async fn release_binary_mysql_cli_attrs_create_database() {
        let Some(bin) = release_server_binary() else {
            eprintln!(
                "skip: build release rusql-server first (`cargo build --release -p rusql-server`)"
            );
            return;
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let data_dir = crate::test_support::temp_data_dir("release_attrs_ms");
        let _ = std::fs::remove_dir_all(&data_dir);

        let mut child = std::process::Command::new(&bin)
            .args([
                "--port",
                &port.to_string(),
                "--data-dir",
                data_dir.to_str().unwrap(),
                "--wal-sync",
                "none",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn release rusql-server");

        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        for _ in 0..120 {
            if tokio::net::TcpStream::connect(addr).await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let mut c1 = crate::test_support::WireClient::connect_addr_like_mysql_cli(addr).await;
        assert!(
            matches!(
                c1.query("CREATE DATABASE app_db").await,
                QueryResponse::Ok { .. }
            ),
            "CREATE DATABASE with query attrs must succeed"
        );
        drop(c1);

        assert_eq!(
            child.try_wait().unwrap(),
            None,
            "release server must stay alive after CREATE DATABASE with query attrs"
        );

        let mut c2 = crate::test_support::WireClient::connect_addr(addr).await;
        assert!(
            matches!(c2.init_db("app_db").await, QueryResponse::Ok { .. }),
            "COM_INIT_DB must work after CREATE DATABASE with query attrs"
        );

        let _ = child.kill();
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// Same environment as `scripts/mysql-diff.mjs`: spawned release server + official mysql CLI.
    #[tokio::test]
    async fn spawned_release_server_official_mysql_multi_schema() {
        if !oracle_mysql_cli_enabled() {
            return;
        }
        let Some(bin) = release_server_binary() else {
            eprintln!(
                "skip: build release rusql-server first (`cargo build --release -p rusql-server`)"
            );
            return;
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let data_dir = crate::test_support::temp_data_dir("release_mysql_cli_ms");
        let _ = std::fs::remove_dir_all(&data_dir);

        let mut child = std::process::Command::new(&bin)
            .args([
                "--port",
                &port.to_string(),
                "--data-dir",
                data_dir.to_str().unwrap(),
                "--wal-sync",
                "none",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn release rusql-server");

        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        for _ in 0..120 {
            if tokio::net::TcpStream::connect(addr).await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let port_s = port.to_string();
        let create = mysql_cli_output(&port_s, &["-B", "-e", "CREATE DATABASE app_db"]).await;
        let create_stderr = String::from_utf8_lossy(&create.stderr);
        assert!(
            create.status.success(),
            "CREATE DATABASE via mysql CLI failed: status={:?} stderr={create_stderr}",
            create.status
        );
        assert_eq!(
            child.try_wait().unwrap(),
            None,
            "server must stay alive after CREATE DATABASE"
        );

        let use_db = mysql_cli_output(&port_s, &["-B", "-D", "app_db", "-e", "SELECT 1"]).await;
        let use_stderr = String::from_utf8_lossy(&use_db.stderr);
        assert!(
            use_db.status.success(),
            "handshake default database app_db via mysql CLI failed: status={:?} stderr={use_stderr}",
            use_db.status
        );

        let _ = child.kill();
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// mysql-diff `multi_schema`: CREATE DATABASE then COM_INIT_DB on a new connection.
    #[tokio::test]
    async fn multi_schema_create_database_then_init_db() {
        let server = TestServer::start("multi_schema_wire").await;

        let mut c1 = server.connect().await;
        assert!(
            matches!(
                c1.query("CREATE DATABASE app_db").await,
                QueryResponse::Ok { .. }
            ),
            "CREATE DATABASE must succeed"
        );
        drop(c1);

        let mut c2 = server.connect().await;
        assert!(
            matches!(c2.init_db("app_db").await, QueryResponse::Ok { .. }),
            "COM_INIT_DB app_db must succeed after CREATE DATABASE"
        );
        assert!(
            matches!(
                c2.query("CREATE TABLE t (id INT PRIMARY KEY)").await,
                QueryResponse::Ok { .. }
            ),
            "DDL in app_db after COM_INIT_DB must succeed"
        );

        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// Oracle gate: default schema via official `mysql` CLI (`-D` → COM_INIT_DB at handshake).
    #[tokio::test]
    async fn official_mysql_client_use_rusql() {
        if !oracle_mysql_cli_enabled() {
            return;
        }

        let server = TestServer::start("official_mysql_use").await;
        let port = server.addr.port().to_string();
        let output = mysql_cli_output(&port, &["-B", "-D", "rusql", "-e", "SELECT 1"]).await;

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "official mysql -D rusql failed: status={:?} stderr={stderr}",
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
        let output = mysqladmin_output(&port, &["ping"]).await;

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

    /// M75: LAST_INSERT_ID() and OK-packet last_insert_id are session-scoped.
    #[tokio::test]
    async fn last_insert_id_session_and_ok_packet() {
        let server = TestServer::start("last_insert_id").await;
        let mut a = server.connect().await;
        let mut b = server.connect().await;

        match a.query("SELECT LAST_INSERT_ID()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["0".to_string()]]);
            }
            other => panic!("expected LAST_INSERT_ID 0, got {other:?}"),
        }

        assert!(matches!(
            a.query("CREATE TABLE li_t (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(16))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            a.query("INSERT INTO li_t (name) VALUES ('alice')").await,
            QueryResponse::Ok { affected_rows: 1 }
        ));
        assert_eq!(a.last_ok_insert_id, 1);

        match a.query("SELECT LAST_INSERT_ID()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected LAST_INSERT_ID 1, got {other:?}"),
        }

        assert!(matches!(
            a.query("INSERT INTO li_t (name) VALUES ('bob'), ('carol')")
                .await,
            QueryResponse::Ok { affected_rows: 2 }
        ));
        assert_eq!(a.last_ok_insert_id, 2);

        match a.query("SELECT LAST_INSERT_ID()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["2".to_string()]]);
            }
            other => panic!("expected LAST_INSERT_ID 2, got {other:?}"),
        }

        match b.query("SELECT LAST_INSERT_ID()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["0".to_string()]],
                    "second connection must not see first connection LAST_INSERT_ID"
                );
            }
            other => panic!("expected isolated LAST_INSERT_ID 0, got {other:?}"),
        }

        assert!(matches!(
            a.reset_connection().await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT LAST_INSERT_ID()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["0".to_string()]]);
            }
            other => panic!("expected LAST_INSERT_ID 0 after reset, got {other:?}"),
        }

        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M76: CONNECTION_ID() matches SHOW PROCESSLIST Id and is unique per connection.
    #[tokio::test]
    async fn connection_id_matches_processlist() {
        let server = TestServer::start("connection_id").await;
        let mut a = server.connect().await;
        let mut b = server.connect().await;

        let id_a = match a.query("SELECT CONNECTION_ID()").await {
            QueryResponse::Rows { rows, .. } => rows[0][0].clone(),
            other => panic!("expected CONNECTION_ID rows, got {other:?}"),
        };
        let id_b = match b.query("SELECT CONNECTION_ID()").await {
            QueryResponse::Rows { rows, .. } => rows[0][0].clone(),
            other => panic!("expected CONNECTION_ID rows, got {other:?}"),
        };
        assert_ne!(
            id_a, id_b,
            "two connections must see different CONNECTION_ID()"
        );

        match a.query("SHOW PROCESSLIST").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Id");
                assert!(
                    rows.iter().any(|r| r[0] == id_a),
                    "PROCESSLIST must include connection A id {id_a}, got {rows:?}"
                );
                assert!(
                    rows.iter().any(|r| r[0] == id_b),
                    "PROCESSLIST must include connection B id {id_b}, got {rows:?}"
                );
            }
            other => panic!("expected PROCESSLIST rows, got {other:?}"),
        }

        match a.process_info().await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows[0][0], id_a,
                    "COM_PROCESS_INFO Id must match CONNECTION_ID()"
                );
            }
            other => panic!("expected process info rows, got {other:?}"),
        }

        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M77: documented @@ session/system variable stubs for client probes.
    #[tokio::test]
    async fn session_var_client_probes() {
        let server = TestServer::start("session_var").await;
        let mut client = server.connect().await;

        match client.query("SELECT @@version").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["@@version".to_string()]);
                assert!(
                    rows[0][0].contains("8.0"),
                    "@@version should be MySQL 8.0-compatible, got {:?}",
                    rows[0][0]
                );
                let version_fn = match client.query("SELECT VERSION()").await {
                    QueryResponse::Rows { rows, .. } => rows[0][0].clone(),
                    other => panic!("expected VERSION() rows, got {other:?}"),
                };
                assert_eq!(rows[0][0], version_fn);
            }
            other => panic!("expected @@version rows, got {other:?}"),
        }

        match client.query("select @@version_comment limit 1").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["@@version_comment".to_string()]);
                assert_eq!(rows.len(), 1);
                assert!(!rows[0][0].is_empty());
            }
            other => panic!("expected @@version_comment rows, got {other:?}"),
        }

        match client
            .query("SELECT @@autocommit, @@session.autocommit")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows[0][0], "1");
                assert_eq!(rows[0][1], "1");
            }
            other => panic!("expected @@autocommit rows, got {other:?}"),
        }

        match client
            .query(
                "SELECT @@character_set_client, @@character_set_connection, @@character_set_results, @@character_set_server, @@collation_connection, @@sql_mode",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows[0][0], "utf8mb4");
                assert_eq!(rows[0][1], "utf8mb4");
                assert_eq!(rows[0][2], "utf8mb4");
                assert_eq!(rows[0][3], "utf8mb4");
                assert_eq!(rows[0][4], "utf8mb4_0900_ai_ci");
                assert!(!rows[0][5].is_empty());
            }
            other => panic!("expected charset/sql_mode rows, got {other:?}"),
        }

        match client
            .query(
                "SELECT @@auto_increment_increment, @@session.auto_increment_increment, @@time_zone, @@system_time_zone, @@transaction_isolation, @@tx_isolation, @@max_allowed_packet, @@license",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows[0][0], "1");
                assert_eq!(rows[0][1], "1");
                assert_eq!(rows[0][2], "SYSTEM");
                assert_eq!(rows[0][3], "UTC");
                assert_eq!(rows[0][4], "REPEATABLE-READ");
                assert_eq!(rows[0][5], "REPEATABLE-READ");
                assert_eq!(rows[0][6], "67108864");
                assert_eq!(rows[0][7], "GPL");
            }
            other => panic!("expected M79 connector probe rows, got {other:?}"),
        }

        match client.query("SELECT @@not_a_real_var").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1193);
                assert!(
                    message.contains("not_a_real_var"),
                    "unknown sysvar message should include the name, got {message}"
                );
            }
            other => panic!("expected errno 1193, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M80: SHOW VARIABLES stub catalog over the documented @@ set.
    #[tokio::test]
    async fn show_variables_stub_catalog() {
        let server = TestServer::start("show_variables").await;
        let mut client = server.connect().await;

        let full = match client.query("SHOW VARIABLES").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec!["Variable_name".to_string(), "Value".to_string()]
                );
                assert!(rows.iter().any(|r| r[0] == "autocommit" && r[1] == "1"));
                assert!(rows
                    .iter()
                    .any(|r| r[0] == "tx_isolation" && r[1] == "REPEATABLE-READ"));
                assert!(rows
                    .iter()
                    .any(|r| r[0] == "version" && r[1].contains("8.0")));
                rows
            }
            other => panic!("expected SHOW VARIABLES rows, got {other:?}"),
        };

        match client.query("SHOW SESSION VARIABLES").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows, full),
            other => panic!("expected SHOW SESSION VARIABLES rows, got {other:?}"),
        }
        match client.query("SHOW GLOBAL VARIABLES").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows, full),
            other => panic!("expected SHOW GLOBAL VARIABLES rows, got {other:?}"),
        }

        match client.query("SHOW VARIABLES LIKE 'auto_increment%'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec![
                        "auto_increment_increment".to_string(),
                        "1".to_string()
                    ]]
                );
            }
            other => panic!("expected LIKE auto_increment% rows, got {other:?}"),
        }

        match client.query("SHOW VARIABLES LIKE 'not_a_real_var%'").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE miss, got {other:?}"),
        }

        match client.query("SELECT @@autocommit").await {
            QueryResponse::Rows { rows, .. } => {
                let show = full.iter().find(|r| r[0] == "autocommit").unwrap();
                assert_eq!(show[1], rows[0][0]);
            }
            other => panic!("expected @@autocommit rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M86: SHOW STATUS stub catalog for client/monitor probes.
    #[tokio::test]
    async fn show_status_stub_catalog() {
        let server = TestServer::start("show_status").await;
        let mut client = server.connect().await;

        let full = match client.query("SHOW STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec!["Variable_name".to_string(), "Value".to_string()]
                );
                assert!(rows.iter().any(|r| r[0] == "Uptime" && r[1] == "0"));
                assert!(rows
                    .iter()
                    .any(|r| r[0] == "Threads_connected" && r[1] == "1"));
                assert!(rows
                    .iter()
                    .any(|r| r[0] == "Threads_running" && r[1] == "1"));
                assert!(rows.iter().any(|r| r[0] == "Questions" && r[1] == "0"));
                rows
            }
            other => panic!("expected SHOW STATUS rows, got {other:?}"),
        };

        match client.query("SHOW SESSION STATUS").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows, full),
            other => panic!("expected SHOW SESSION STATUS rows, got {other:?}"),
        }
        match client.query("SHOW GLOBAL STATUS").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows, full),
            other => panic!("expected SHOW GLOBAL STATUS rows, got {other:?}"),
        }

        match client.query("SHOW STATUS LIKE 'Threads%'").await {
            QueryResponse::Rows { rows, .. } => {
                let names: Vec<_> = rows.iter().map(|r| r[0].as_str()).collect();
                assert_eq!(names, vec!["Threads_connected", "Threads_running"]);
            }
            other => panic!("expected LIKE Threads% rows, got {other:?}"),
        }

        match client.query("SHOW STATUS LIKE 'not_a_real_status%'").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE miss, got {other:?}"),
        }

        let mut second = server.connect().await;
        match client.query("SHOW STATUS LIKE 'Threads_connected'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["Threads_connected".to_string(), "2".to_string()]]
                );
            }
            other => panic!("expected Threads_connected=2, got {other:?}"),
        }

        second.quit().await;
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M87: SHOW TABLE STATUS stubs with real names and LIKE.
    #[tokio::test]
    async fn show_table_status_stubs() {
        let server = TestServer::start("show_table_status").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE TABLE sts_a (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(16))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE TABLE sts_b (id INT PRIMARY KEY)")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO sts_a (name) VALUES ('x')").await,
            QueryResponse::Ok { .. }
        ));

        let names = match client.query("SHOW TABLE STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Name");
                assert!(columns.contains(&"Engine".to_string()));
                assert!(columns.contains(&"Rows".to_string()));
                assert!(columns.contains(&"Collation".to_string()));
                assert!(columns.contains(&"Comment".to_string()));
                let names: Vec<_> = rows.iter().map(|r| r[0].as_str()).collect();
                assert!(names.contains(&"sts_a"));
                assert!(names.contains(&"sts_b"));
                let a = rows.iter().find(|r| r[0] == "sts_a").unwrap();
                assert_eq!(a[1], "InnoDB");
                assert_eq!(a[4], "1");
                names.into_iter().map(str::to_string).collect::<Vec<_>>()
            }
            other => panic!("expected SHOW TABLE STATUS rows, got {other:?}"),
        };

        match client.query("SHOW TABLES").await {
            QueryResponse::Rows { rows, .. } => {
                let tables: Vec<_> = rows.iter().map(|r| r[0].clone()).collect();
                for name in &names {
                    assert!(
                        tables.contains(name),
                        "SHOW TABLES should include {name}, got {tables:?}"
                    );
                }
            }
            other => panic!("expected SHOW TABLES rows, got {other:?}"),
        }

        match client.query("SHOW TABLE STATUS LIKE 'sts_a%'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "sts_a");
            }
            other => panic!("expected LIKE sts_a% rows, got {other:?}"),
        }
        match client.query("SHOW TABLE STATUS LIKE 'no_such%'").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE miss, got {other:?}"),
        }

        match client.query("SHOW STATUS LIKE 'Uptime'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["Uptime".to_string(), "0".to_string()]]);
            }
            other => panic!("SHOW STATUS must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M88: SHOW ENGINES / SHOW STORAGE ENGINES documented stub catalog.
    #[tokio::test]
    async fn show_engines_stubs() {
        let server = TestServer::start("show_engines").await;
        let mut client = server.connect().await;

        let full = match client.query("SHOW ENGINES").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Engine");
                assert!(columns.contains(&"Support".to_string()));
                assert!(columns.contains(&"Comment".to_string()));
                assert!(columns.contains(&"Transactions".to_string()));
                assert!(columns.contains(&"XA".to_string()));
                assert!(columns.contains(&"Savepoints".to_string()));
                let innodb = rows.iter().find(|r| r[0] == "InnoDB").expect("InnoDB");
                assert_eq!(innodb[1], "DEFAULT");
                assert!(rows.iter().any(|r| r[0] == "MEMORY" && r[1] == "YES"));
                assert!(rows.iter().any(|r| r[0] == "MyISAM" && r[1] == "YES"));
                assert!(rows
                    .iter()
                    .any(|r| r[0] == "PERFORMANCE_SCHEMA" && r[1] == "YES"));
                rows
            }
            other => panic!("expected SHOW ENGINES rows, got {other:?}"),
        };

        match client.query("SHOW STORAGE ENGINES").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows, full),
            other => panic!("expected SHOW STORAGE ENGINES rows, got {other:?}"),
        }

        match client.query("SHOW ENGINE INNODB STATUS").await {
            QueryResponse::Err { .. } => {}
            other => panic!("SHOW ENGINE INNODB STATUS should error, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE TABLE eng_t (id INT PRIMARY KEY)")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW TABLE STATUS LIKE 'eng_t'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "eng_t");
                assert_eq!(rows[0][1], "InnoDB");
            }
            other => panic!("SHOW TABLE STATUS must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW STATUS LIKE 'Uptime'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["Uptime".to_string(), "0".to_string()]]);
            }
            other => panic!("SHOW STATUS must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M89: SHOW CHARACTER SET / SHOW CHARSET documented stub catalog.
    #[tokio::test]
    async fn show_character_set_stubs() {
        let server = TestServer::start("show_character_set").await;
        let mut client = server.connect().await;

        let full = match client.query("SHOW CHARACTER SET").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Charset");
                assert!(columns.contains(&"Description".to_string()));
                assert!(columns.contains(&"Default collation".to_string()));
                assert!(columns.contains(&"Maxlen".to_string()));
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "utf8mb4");
                assert_eq!(rows[0][2], "utf8mb4_unicode_ci");
                assert_eq!(rows[0][3], "4");
                rows
            }
            other => panic!("expected SHOW CHARACTER SET rows, got {other:?}"),
        };

        match client.query("SHOW CHARSET").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows, full),
            other => panic!("expected SHOW CHARSET rows, got {other:?}"),
        }
        match client.query("SHOW CHARACTER SET LIKE 'utf8%'").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows, full),
            other => panic!("expected LIKE utf8% rows, got {other:?}"),
        }
        match client
            .query("SHOW CHARACTER SET LIKE 'no_such_charset%'")
            .await
        {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE miss, got {other:?}"),
        }

        match client.query("SHOW ENGINES").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows.iter().any(|r| r[0] == "InnoDB" && r[1] == "DEFAULT"));
            }
            other => panic!("SHOW ENGINES must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW COLLATION").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows.iter().any(|r| r[0] == "utf8mb4_unicode_ci"));
            }
            other => panic!("SHOW COLLATION must stay unchanged, got {other:?}"),
        }
        assert!(matches!(
            client.query("SET CHARACTER SET utf8mb4").await,
            QueryResponse::Ok { .. }
        ));

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M90: SHOW WARNINGS / SHOW ERRORS documented empty diagnostic list.
    #[tokio::test]
    async fn show_warnings_stubs() {
        let server = TestServer::start("show_warnings").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client.query("SELECT 1").await,
            QueryResponse::Rows { .. }
        ));

        match client.query("SHOW WARNINGS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Level");
                assert!(columns.contains(&"Code".to_string()));
                assert!(columns.contains(&"Message".to_string()));
                assert!(rows.is_empty());
            }
            other => panic!("expected empty SHOW WARNINGS rows, got {other:?}"),
        }

        match client.query("SHOW ERRORS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Level");
                assert!(columns.contains(&"Code".to_string()));
                assert!(columns.contains(&"Message".to_string()));
                assert!(rows.is_empty());
            }
            other => panic!("expected empty SHOW ERRORS rows, got {other:?}"),
        }

        match client.query("SHOW CHARACTER SET").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "utf8mb4");
            }
            other => panic!("SHOW CHARACTER SET must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW ENGINES").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows.iter().any(|r| r[0] == "InnoDB" && r[1] == "DEFAULT"));
            }
            other => panic!("SHOW ENGINES must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M91: SHOW CREATE DATABASE / SHOW CREATE SCHEMA documented stub DDL.
    #[tokio::test]
    async fn show_create_database_stubs() {
        let server = TestServer::start("show_create_database").await;
        let mut client = server.connect().await;

        match client.query("SHOW CREATE DATABASE rusql").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Database");
                assert!(columns.contains(&"Create Database".to_string()));
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "rusql");
                assert!(rows[0][1].contains("CREATE DATABASE `rusql`"));
                assert!(rows[0][1].contains("utf8mb4"));
                assert!(rows[0][1].contains("utf8mb4_unicode_ci"));
            }
            other => panic!("expected SHOW CREATE DATABASE rows, got {other:?}"),
        }

        assert!(matches!(
            client.query("CREATE DATABASE app_db").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE SCHEMA app_db").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Database");
                assert_eq!(rows[0][0], "app_db");
                assert!(rows[0][1].contains("CREATE DATABASE `app_db`"));
                assert!(rows[0][1].contains("utf8mb4"));
                assert!(rows[0][1].contains("utf8mb4_unicode_ci"));
            }
            other => panic!("expected SHOW CREATE SCHEMA rows, got {other:?}"),
        }

        assert!(matches!(
            client.query("SHOW CREATE DATABASE no_such_db").await,
            QueryResponse::Err { code: 1049, .. }
        ));

        assert!(matches!(
            client
                .query("CREATE TABLE items (id INT, label VARCHAR(16))")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE TABLE items").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Table");
                assert_eq!(rows[0][0], "items");
                assert!(rows[0][1].contains("CREATE TABLE `items`"));
            }
            other => panic!("SHOW CREATE TABLE must stay unchanged, got {other:?}"),
        }

        match client.query("SHOW WARNINGS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Level");
                assert!(rows.is_empty());
            }
            other => panic!("SHOW WARNINGS must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M92: SHOW CREATE VIEW reconstructed from catalog SELECT.
    #[tokio::test]
    async fn show_create_view_stubs() {
        let server = TestServer::start("show_create_view").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE TABLE vt (id INT, label VARCHAR(16))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("CREATE VIEW v_ids AS SELECT id FROM vt").await,
            QueryResponse::Ok { .. }
        ));

        match client.query("SHOW CREATE VIEW v_ids").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "View");
                assert!(columns.contains(&"Create View".to_string()));
                assert!(columns.contains(&"character_set_client".to_string()));
                assert_eq!(rows[0][0], "v_ids");
                assert!(rows[0][1].contains("CREATE VIEW `v_ids` AS"));
                assert!(rows[0][1].contains("SELECT"));
                assert_eq!(rows[0][2], "utf8mb4");
                assert_eq!(rows[0][3], "utf8mb4_unicode_ci");
            }
            other => panic!("expected SHOW CREATE VIEW rows, got {other:?}"),
        }

        assert!(matches!(
            client.query("SHOW CREATE VIEW no_such_view").await,
            QueryResponse::Err { code: 1146, .. }
        ));

        match client.query("SHOW CREATE TABLE vt").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Table");
                assert_eq!(rows[0][0], "vt");
                assert!(rows[0][1].contains("CREATE TABLE `vt`"));
            }
            other => panic!("SHOW CREATE TABLE must stay unchanged, got {other:?}"),
        }

        match client.query("SHOW CREATE DATABASE rusql").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Database");
                assert_eq!(rows[0][0], "rusql");
            }
            other => panic!("SHOW CREATE DATABASE must stay unchanged, got {other:?}"),
        }

        match client.query("SHOW WARNINGS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Level");
                assert!(rows.is_empty());
            }
            other => panic!("SHOW WARNINGS must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M93: SHOW TRIGGERS lists catalog TriggerMeta with documented stub cells.
    #[tokio::test]
    async fn show_triggers_stubs() {
        let server = TestServer::start("show_triggers").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client.query("CREATE TABLE src (id INT PRIMARY KEY)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query(
                    "CREATE TRIGGER tr_src BEFORE INSERT ON src FOR EACH ROW SET NEW.id = NEW.id"
                )
                .await,
            QueryResponse::Ok { .. }
        ));

        match client.query("SHOW TRIGGERS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Trigger");
                assert!(columns.contains(&"Event".to_string()));
                assert!(columns.contains(&"Table".to_string()));
                assert!(columns.contains(&"Statement".to_string()));
                assert!(columns.contains(&"Timing".to_string()));
                assert!(columns.contains(&"Definer".to_string()));
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "tr_src");
                assert_eq!(rows[0][1], "INSERT");
                assert_eq!(rows[0][2], "src");
                assert!(rows[0][3].contains("SET NEW.id"));
                assert_eq!(rows[0][4], "BEFORE");
                assert_eq!(rows[0][8], "utf8mb4");
                assert_eq!(rows[0][9], "utf8mb4_unicode_ci");
            }
            other => panic!("expected SHOW TRIGGERS rows, got {other:?}"),
        }

        match client.query("SHOW TRIGGERS LIKE 'tr_s%'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "tr_src");
            }
            other => panic!("expected SHOW TRIGGERS LIKE rows, got {other:?}"),
        }
        match client.query("SHOW TRIGGERS LIKE 'no_such%'").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty SHOW TRIGGERS LIKE, got {other:?}"),
        }

        assert!(matches!(
            client.query("CREATE DATABASE trg_db").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW TRIGGERS FROM trg_db").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty SHOW TRIGGERS FROM trg_db, got {other:?}"),
        }
        match client.query("SHOW TRIGGERS FROM missing_db").await {
            QueryResponse::Err { code: 1049, .. } => {}
            other => panic!("expected errno 1049 for unknown FROM db, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE VIEW v_ids AS SELECT id FROM src")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE VIEW v_ids").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "View");
                assert_eq!(rows[0][0], "v_ids");
                assert!(rows[0][1].contains("CREATE VIEW `v_ids` AS"));
            }
            other => panic!("SHOW CREATE VIEW must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW CREATE TABLE src").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Table");
                assert_eq!(rows[0][0], "src");
                assert!(rows[0][1].contains("CREATE TABLE `src`"));
            }
            other => panic!("SHOW CREATE TABLE must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW CREATE DATABASE rusql").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Database");
                assert_eq!(rows[0][0], "rusql");
            }
            other => panic!("SHOW CREATE DATABASE must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M94: SHOW CREATE TRIGGER reconstructs catalog TriggerMeta DDL.
    #[tokio::test]
    async fn show_create_trigger_stubs() {
        let server = TestServer::start("show_create_trigger").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client.query("CREATE TABLE src (id INT PRIMARY KEY)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query(
                    "CREATE TRIGGER tr_src BEFORE INSERT ON src FOR EACH ROW SET NEW.id = NEW.id"
                )
                .await,
            QueryResponse::Ok { .. }
        ));

        match client.query("SHOW CREATE TRIGGER tr_src").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Trigger");
                assert!(columns.contains(&"sql_mode".to_string()));
                assert!(columns.contains(&"SQL Original Statement".to_string()));
                assert!(columns.contains(&"character_set_client".to_string()));
                assert!(columns.contains(&"collation_connection".to_string()));
                assert!(columns.contains(&"Database Collation".to_string()));
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "tr_src");
                assert!(rows[0][2].contains("CREATE TRIGGER `tr_src`"));
                assert!(rows[0][2].contains("BEFORE INSERT ON `src`"));
                assert!(rows[0][2].contains("FOR EACH ROW"));
                assert!(rows[0][2].contains("SET NEW.id"));
                assert!(!rows[0][2].contains("DEFINER"));
                assert_eq!(rows[0][3], "utf8mb4");
                assert_eq!(rows[0][4], "utf8mb4_unicode_ci");
            }
            other => panic!("expected SHOW CREATE TRIGGER rows, got {other:?}"),
        }

        match client.query("SHOW CREATE TRIGGER no_such_trigger").await {
            QueryResponse::Err { code: 1360, .. } => {}
            other => panic!("expected errno 1360 for unknown trigger, got {other:?}"),
        }

        match client.query("SHOW TRIGGERS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Trigger");
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "tr_src");
                assert_eq!(rows[0][1], "INSERT");
                assert_eq!(rows[0][7], "root@%");
            }
            other => panic!("SHOW TRIGGERS must stay unchanged, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE VIEW v_ids AS SELECT id FROM src")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE VIEW v_ids").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "View");
                assert_eq!(rows[0][0], "v_ids");
                assert!(rows[0][1].contains("CREATE VIEW `v_ids` AS"));
            }
            other => panic!("SHOW CREATE VIEW must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW CREATE TABLE src").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Table");
                assert_eq!(rows[0][0], "src");
                assert!(rows[0][1].contains("CREATE TABLE `src`"));
            }
            other => panic!("SHOW CREATE TABLE must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M95: SHOW CREATE PROCEDURE reconstructs catalog ProcedureMeta DDL.
    #[tokio::test]
    async fn show_create_procedure_stubs() {
        let server = TestServer::start("show_create_procedure").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client.query("CREATE TABLE src (id INT PRIMARY KEY)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE PROCEDURE p() BEGIN INSERT INTO src VALUES (42); END")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query(
                    "CREATE TRIGGER tr_src BEFORE INSERT ON src FOR EACH ROW SET NEW.id = NEW.id"
                )
                .await,
            QueryResponse::Ok { .. }
        ));

        match client.query("SHOW CREATE PROCEDURE p").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Procedure");
                assert!(columns.contains(&"sql_mode".to_string()));
                assert!(columns.contains(&"Create Procedure".to_string()));
                assert!(columns.contains(&"character_set_client".to_string()));
                assert!(columns.contains(&"collation_connection".to_string()));
                assert!(columns.contains(&"Database Collation".to_string()));
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "p");
                assert!(rows[0][2].contains("CREATE PROCEDURE `p`()"));
                assert!(rows[0][2].contains("BEGIN"));
                assert!(rows[0][2].contains("INSERT INTO src VALUES (42)"));
                assert!(rows[0][2].contains("END"));
                assert!(!rows[0][2].contains("DEFINER"));
                assert_eq!(rows[0][3], "utf8mb4");
                assert_eq!(rows[0][4], "utf8mb4_unicode_ci");
            }
            other => panic!("expected SHOW CREATE PROCEDURE rows, got {other:?}"),
        }

        match client.query("SHOW CREATE PROCEDURE no_such_proc").await {
            QueryResponse::Err { code: 1305, .. } => {}
            other => panic!("expected errno 1305 for unknown procedure, got {other:?}"),
        }

        match client.query("SHOW CREATE TRIGGER tr_src").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Trigger");
                assert!(rows[0][2].contains("CREATE TRIGGER `tr_src`"));
            }
            other => panic!("SHOW CREATE TRIGGER must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW TRIGGERS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Trigger");
                assert_eq!(rows[0][0], "tr_src");
            }
            other => panic!("SHOW TRIGGERS must stay unchanged, got {other:?}"),
        }
        assert!(matches!(
            client
                .query("CREATE VIEW v_ids AS SELECT id FROM src")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE VIEW v_ids").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "View");
                assert!(rows[0][1].contains("CREATE VIEW `v_ids` AS"));
            }
            other => panic!("SHOW CREATE VIEW must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M96: SHOW CREATE FUNCTION reconstructs catalog FunctionMeta DDL.
    #[tokio::test]
    async fn show_create_function_stubs() {
        let server = TestServer::start("show_create_function").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client.query("CREATE TABLE src (id INT PRIMARY KEY)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE FUNCTION f() RETURNS INT BEGIN RETURN 42; END")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE PROCEDURE p() BEGIN INSERT INTO src VALUES (42); END")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query(
                    "CREATE TRIGGER tr_src BEFORE INSERT ON src FOR EACH ROW SET NEW.id = NEW.id"
                )
                .await,
            QueryResponse::Ok { .. }
        ));

        match client.query("SHOW CREATE FUNCTION f").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Function");
                assert!(columns.contains(&"sql_mode".to_string()));
                assert!(columns.contains(&"Create Function".to_string()));
                assert!(columns.contains(&"character_set_client".to_string()));
                assert!(columns.contains(&"collation_connection".to_string()));
                assert!(columns.contains(&"Database Collation".to_string()));
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "f");
                assert!(rows[0][2].contains("CREATE FUNCTION `f`()"));
                assert!(rows[0][2].contains("RETURNS INT"));
                assert!(rows[0][2].contains("BEGIN RETURN 42; END"));
                assert!(!rows[0][2].contains("DEFINER"));
                assert_eq!(rows[0][3], "utf8mb4");
                assert_eq!(rows[0][4], "utf8mb4_unicode_ci");
            }
            other => panic!("expected SHOW CREATE FUNCTION rows, got {other:?}"),
        }

        match client.query("SHOW CREATE FUNCTION no_such_fn").await {
            QueryResponse::Err { code: 1305, .. } => {}
            other => panic!("expected errno 1305 for unknown function, got {other:?}"),
        }

        match client.query("SHOW CREATE PROCEDURE p").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Procedure");
                assert!(rows[0][2].contains("CREATE PROCEDURE `p`()"));
            }
            other => panic!("SHOW CREATE PROCEDURE must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW CREATE TRIGGER tr_src").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Trigger");
                assert!(rows[0][2].contains("CREATE TRIGGER `tr_src`"));
            }
            other => panic!("SHOW CREATE TRIGGER must stay unchanged, got {other:?}"),
        }
        assert!(matches!(
            client
                .query("CREATE VIEW v_ids AS SELECT id FROM src")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE VIEW v_ids").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "View");
                assert!(rows[0][1].contains("CREATE VIEW `v_ids` AS"));
            }
            other => panic!("SHOW CREATE VIEW must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M81: SET @@ / SET SESSION overlays persist per connection.
    #[tokio::test]
    async fn set_session_var_persists_and_resets() {
        let server = TestServer::start("set_session_var").await;
        let mut a = server.connect().await;
        let mut b = server.connect().await;

        assert!(matches!(
            a.query("SET @@autocommit = 0").await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@autocommit").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["0".to_string()]]);
            }
            other => panic!("expected @@autocommit 0 after SET, got {other:?}"),
        }
        match a.query("SELECT @@session.autocommit").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["0".to_string()]]);
            }
            other => panic!("expected @@session.autocommit 0, got {other:?}"),
        }
        match b.query("SELECT @@autocommit").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected isolated @@autocommit 1, got {other:?}"),
        }

        match a.query("SHOW SESSION VARIABLES LIKE 'autocommit'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["autocommit".to_string(), "0".to_string()]]);
            }
            other => panic!("expected SHOW SESSION overlay, got {other:?}"),
        }
        match a.query("SHOW GLOBAL VARIABLES LIKE 'autocommit'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["autocommit".to_string(), "1".to_string()]]);
            }
            other => panic!("expected SHOW GLOBAL default, got {other:?}"),
        }

        assert!(matches!(
            a.query("SET SESSION autocommit = 1").await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@autocommit").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected autocommit restored to 1, got {other:?}"),
        }
        assert!(matches!(
            a.query("SET @@session.autocommit = 0").await,
            QueryResponse::Ok { .. }
        ));

        match a.query("SET GLOBAL autocommit = 0").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1229);
                assert!(message.contains("autocommit"));
            }
            other => panic!("expected SET GLOBAL errno 1229, got {other:?}"),
        }
        match a.query("SET @@version = 'nope'").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1238);
                assert!(message.contains("version"));
            }
            other => panic!("expected read-only errno 1238, got {other:?}"),
        }
        match a.query("SET @@not_a_real_var = 1").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1193);
                assert!(message.contains("not_a_real_var"));
            }
            other => panic!("expected unknown SET errno 1193, got {other:?}"),
        }

        assert!(matches!(
            a.reset_connection().await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@autocommit").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected autocommit default after reset, got {other:?}"),
        }

        assert!(matches!(
            a.query("SET @@autocommit = 0").await,
            QueryResponse::Ok { .. }
        ));
        a.change_user("root", "", "rusql").await;
        match a.query("SELECT @@autocommit").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected autocommit default after CHANGE_USER, got {other:?}"),
        }

        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M82: SET NAMES overlays charset stubs; @foo is session-scoped.
    #[tokio::test]
    async fn set_names_and_user_var_persist_and_reset() {
        let server = TestServer::start("set_names_user_var").await;
        let mut a = server.connect().await;
        let mut b = server.connect().await;

        assert!(matches!(
            a.query("SET NAMES utf8mb4").await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@character_set_client").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["utf8mb4".to_string()]]);
            }
            other => panic!("expected charset_client utf8mb4, got {other:?}"),
        }

        assert!(matches!(
            a.query("SET NAMES utf8mb4 COLLATE utf8mb4_unicode_ci")
                .await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@collation_connection").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["utf8mb4_unicode_ci".to_string()]]);
            }
            other => panic!("expected collation_connection overlay, got {other:?}"),
        }

        assert!(matches!(
            a.query("SET NAMES DEFAULT").await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@collation_connection").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["utf8mb4_0900_ai_ci".to_string()]]);
            }
            other => panic!("expected collation default after SET NAMES DEFAULT, got {other:?}"),
        }

        assert!(matches!(
            a.query("SET @foo = 1").await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @foo").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected @foo 1, got {other:?}"),
        }
        match b.query("SELECT @foo").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["".to_string()]]);
            }
            other => panic!("expected isolated unset @foo, got {other:?}"),
        }
        match a.query("SELECT @bar").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["".to_string()]]);
            }
            other => panic!("expected unset @bar empty, got {other:?}"),
        }

        assert!(matches!(
            a.reset_connection().await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @foo").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["".to_string()]]);
            }
            other => panic!("expected @foo cleared after reset, got {other:?}"),
        }

        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M83: SET CHARACTER SET / SET CHARSET alias SET NAMES; SELECT @foo := expr.
    #[tokio::test]
    async fn set_charset_and_user_var_assign_persist_and_reset() {
        let server = TestServer::start("set_charset_user_var_assign").await;
        let mut a = server.connect().await;
        let mut b = server.connect().await;

        assert!(matches!(
            a.query("SET CHARACTER SET utf8mb4").await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@character_set_client").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["utf8mb4".to_string()]]);
            }
            other => panic!("expected charset_client utf8mb4, got {other:?}"),
        }
        match a.query("SELECT @@character_set_connection").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["utf8mb4".to_string()]]);
            }
            other => panic!("expected charset_connection utf8mb4, got {other:?}"),
        }
        match a.query("SELECT @@character_set_results").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["utf8mb4".to_string()]]);
            }
            other => panic!("expected charset_results utf8mb4, got {other:?}"),
        }

        assert!(matches!(
            a.query("SET CHARSET utf8mb4").await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@character_set_client").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["utf8mb4".to_string()]]);
            }
            other => panic!("expected CHARSET overlay, got {other:?}"),
        }

        match a.query("SELECT @foo := 1").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected @foo := 1 result 1, got {other:?}"),
        }
        match a.query("SELECT @foo").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected @foo 1 after assign, got {other:?}"),
        }
        match b.query("SELECT @foo").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["".to_string()]]);
            }
            other => panic!("expected isolated unset @foo, got {other:?}"),
        }
        match a.query("SELECT @bar").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["".to_string()]]);
            }
            other => panic!("expected unset @bar empty, got {other:?}"),
        }

        assert!(matches!(
            a.reset_connection().await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @foo").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["".to_string()]]);
            }
            other => panic!("expected @foo cleared after reset, got {other:?}"),
        }
        match a.query("SELECT @@character_set_client").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["utf8mb4".to_string()]]);
            }
            other => panic!("expected charset stub after reset, got {other:?}"),
        }

        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M84: SET TRANSACTION ISOLATION LEVEL overlays @@transaction_isolation / @@tx_isolation.
    #[tokio::test]
    async fn set_transaction_isolation_persists_and_resets() {
        let server = TestServer::start("set_transaction_isolation").await;
        let mut a = server.connect().await;
        let mut b = server.connect().await;

        assert!(matches!(
            a.query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
                .await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@transaction_isolation").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["READ-COMMITTED".to_string()]]);
            }
            other => panic!("expected @@transaction_isolation READ-COMMITTED, got {other:?}"),
        }
        match a.query("SELECT @@tx_isolation").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["READ-COMMITTED".to_string()]]);
            }
            other => panic!("expected @@tx_isolation READ-COMMITTED, got {other:?}"),
        }
        match b.query("SELECT @@transaction_isolation").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["REPEATABLE-READ".to_string()]]);
            }
            other => panic!("expected isolated default isolation, got {other:?}"),
        }

        assert!(matches!(
            a.query("SET SESSION TRANSACTION ISOLATION LEVEL REPEATABLE READ")
                .await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@transaction_isolation").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["REPEATABLE-READ".to_string()]]);
            }
            other => panic!("expected REPEATABLE-READ after SESSION SET, got {other:?}"),
        }

        assert!(matches!(
            a.query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@transaction_isolation").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["SERIALIZABLE".to_string()]]);
            }
            other => panic!("expected SERIALIZABLE, got {other:?}"),
        }
        assert!(matches!(
            a.query("SET TRANSACTION ISOLATION LEVEL READ UNCOMMITTED")
                .await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@tx_isolation").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["READ-UNCOMMITTED".to_string()]]);
            }
            other => panic!("expected READ-UNCOMMITTED, got {other:?}"),
        }

        match a
            .query("SHOW SESSION VARIABLES LIKE 'transaction_isolation'")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec![
                        "transaction_isolation".to_string(),
                        "READ-UNCOMMITTED".to_string()
                    ]]
                );
            }
            other => panic!("expected SHOW SESSION overlay, got {other:?}"),
        }
        match a
            .query("SHOW GLOBAL VARIABLES LIKE 'transaction_isolation'")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec![
                        "transaction_isolation".to_string(),
                        "REPEATABLE-READ".to_string()
                    ]]
                );
            }
            other => panic!("expected SHOW GLOBAL default, got {other:?}"),
        }

        match a
            .query("SET GLOBAL TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .await
        {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1229);
                assert!(message.contains("transaction_isolation"));
            }
            other => panic!("expected SET GLOBAL TRANSACTION errno 1229, got {other:?}"),
        }

        assert!(matches!(
            a.reset_connection().await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT @@transaction_isolation").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["REPEATABLE-READ".to_string()]]);
            }
            other => panic!("expected isolation default after reset, got {other:?}"),
        }

        assert!(matches!(
            a.query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
                .await,
            QueryResponse::Ok { .. }
        ));
        a.change_user("root", "", "rusql").await;
        match a.query("SELECT @@tx_isolation").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["REPEATABLE-READ".to_string()]]);
            }
            other => panic!("expected isolation default after CHANGE_USER, got {other:?}"),
        }

        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M85: SELECT … FOR UPDATE / FOR SHARE is a documented snapshot no-op.
    #[tokio::test]
    async fn select_for_update_is_unlocked_noop() {
        let server = TestServer::start("select_for_update").await;
        let mut setup = server.connect().await;

        assert!(matches!(
            setup
                .query("CREATE TABLE fu_t (id INT PRIMARY KEY, name VARCHAR(16))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            setup.query("INSERT INTO fu_t VALUES (1, 'a')").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            setup.query("INSERT INTO fu_t VALUES (2, 'b')").await,
            QueryResponse::Ok { .. }
        ));
        setup.quit().await;

        let mut a = server.connect().await;
        let mut b = server.connect().await;

        let unlocked = match a.query("SELECT id FROM fu_t ORDER BY id").await {
            QueryResponse::Rows { rows, .. } => rows,
            other => panic!("expected unlocked rows, got {other:?}"),
        };
        assert_eq!(unlocked, vec![vec!["1".to_string()], vec!["2".to_string()]]);

        for sql in [
            "SELECT id FROM fu_t ORDER BY id FOR UPDATE",
            "SELECT id FROM fu_t ORDER BY id FOR SHARE",
            "SELECT id FROM fu_t ORDER BY id LOCK IN SHARE MODE",
            "SELECT id FROM fu_t ORDER BY id FOR UPDATE NOWAIT",
            "SELECT id FROM fu_t ORDER BY id FOR UPDATE SKIP LOCKED",
        ] {
            match a.query(sql).await {
                QueryResponse::Rows { rows, .. } => {
                    assert_eq!(rows, unlocked, "lock clause should be a no-op for {sql}");
                }
                other => panic!("expected rows for {sql}, got {other:?}"),
            }
        }

        assert!(matches!(a.query("BEGIN").await, QueryResponse::Ok { .. }));
        assert!(matches!(b.query("BEGIN").await, QueryResponse::Ok { .. }));
        match a.query("SELECT id FROM fu_t WHERE id = 1 FOR UPDATE").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected FOR UPDATE row, got {other:?}"),
        }
        match b.query("SELECT id FROM fu_t WHERE id = 1 FOR UPDATE").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["1".to_string()]],
                    "concurrent FOR UPDATE must not block"
                );
            }
            other => panic!("expected concurrent FOR UPDATE row, got {other:?}"),
        }
        assert!(matches!(a.query("COMMIT").await, QueryResponse::Ok { .. }));
        assert!(matches!(b.query("COMMIT").await, QueryResponse::Ok { .. }));

        assert!(matches!(
            a.query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
                .await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT id FROM fu_t WHERE id = 1 FOR UPDATE").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected FOR UPDATE after SET TRANSACTION, got {other:?}"),
        }
        match a.query("SELECT @@transaction_isolation").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["READ-COMMITTED".to_string()]]);
            }
            other => panic!("expected isolation overlay unchanged, got {other:?}"),
        }
        match b.query("SELECT @@transaction_isolation").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["REPEATABLE-READ".to_string()]]);
            }
            other => panic!("expected other connection default isolation, got {other:?}"),
        }

        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn found_rows_plain_and_sql_calc() {
        let server = TestServer::start("found_rows").await;
        let mut a = server.connect().await;
        let mut b = server.connect().await;

        match a.query("SELECT FOUND_ROWS()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["0".to_string()]]);
            }
            other => panic!("expected FOUND_ROWS 0, got {other:?}"),
        }

        for sql in [
            "CREATE TABLE fr_t (id INT PRIMARY KEY, name VARCHAR(16))",
            "INSERT INTO fr_t VALUES (1, 'a')",
            "INSERT INTO fr_t VALUES (2, 'b')",
            "INSERT INTO fr_t VALUES (3, 'c')",
        ] {
            assert!(matches!(a.query(sql).await, QueryResponse::Ok { .. }));
        }

        assert!(matches!(
            a.query("SELECT id FROM fr_t LIMIT 1").await,
            QueryResponse::Rows { .. }
        ));
        match a.query("SELECT FOUND_ROWS()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected FOUND_ROWS 1, got {other:?}"),
        }

        assert!(matches!(
            a.query("SELECT SQL_CALC_FOUND_ROWS id FROM fr_t LIMIT 1")
                .await,
            QueryResponse::Rows { .. }
        ));
        match a.query("SELECT FOUND_ROWS()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["3".to_string()]]);
            }
            other => panic!("expected FOUND_ROWS 3 after SQL_CALC, got {other:?}"),
        }

        match b.query("SELECT FOUND_ROWS()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["0".to_string()]]);
            }
            other => panic!("expected isolated FOUND_ROWS 0, got {other:?}"),
        }

        assert!(matches!(
            a.reset_connection().await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT FOUND_ROWS()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["0".to_string()]]);
            }
            other => panic!("expected FOUND_ROWS 0 after reset, got {other:?}"),
        }

        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M76: ROW_COUNT() is session-scoped and follows MySQL SELECT = -1 semantics.
    #[tokio::test]
    async fn row_count_session_after_dml_and_select() {
        let server = TestServer::start("row_count").await;
        let mut a = server.connect().await;
        let mut b = server.connect().await;

        match a.query("SELECT ROW_COUNT()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["-1".to_string()]]);
            }
            other => panic!("expected ROW_COUNT -1, got {other:?}"),
        }

        assert!(matches!(
            a.query("CREATE TABLE rc_t (id INT PRIMARY KEY, name VARCHAR(16))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            a.query("INSERT INTO rc_t (id, name) VALUES (1, 'alice')")
                .await,
            QueryResponse::Ok { affected_rows: 1 }
        ));
        match a.query("SELECT ROW_COUNT()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected ROW_COUNT 1 after INSERT, got {other:?}"),
        }

        assert!(matches!(
            a.query("INSERT INTO rc_t (id, name) VALUES (2, 'bob'), (3, 'carol')")
                .await,
            QueryResponse::Ok { affected_rows: 2 }
        ));
        match a.query("SELECT ROW_COUNT()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["2".to_string()]]);
            }
            other => panic!("expected ROW_COUNT 2 after multi INSERT, got {other:?}"),
        }

        assert!(matches!(
            a.query("UPDATE rc_t SET name = 'Alice' WHERE id = 1").await,
            QueryResponse::Ok { affected_rows: 1 }
        ));
        match a.query("SELECT ROW_COUNT()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected ROW_COUNT 1 after UPDATE, got {other:?}"),
        }

        assert!(matches!(
            a.query("SELECT name FROM rc_t WHERE id = 1").await,
            QueryResponse::Rows { .. }
        ));
        match a.query("SELECT ROW_COUNT()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["-1".to_string()]]);
            }
            other => panic!("expected ROW_COUNT -1 after SELECT, got {other:?}"),
        }

        assert!(matches!(
            a.query("DELETE FROM rc_t WHERE id = 2").await,
            QueryResponse::Ok { affected_rows: 1 }
        ));
        match a.query("SELECT ROW_COUNT()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected ROW_COUNT 1 after DELETE, got {other:?}"),
        }

        match b.query("SELECT ROW_COUNT()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["-1".to_string()]],
                    "second connection must not see first connection ROW_COUNT"
                );
            }
            other => panic!("expected isolated ROW_COUNT -1, got {other:?}"),
        }

        assert!(matches!(
            a.reset_connection().await,
            QueryResponse::Ok { .. }
        ));
        match a.query("SELECT ROW_COUNT()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["-1".to_string()]]);
            }
            other => panic!("expected ROW_COUNT -1 after reset, got {other:?}"),
        }

        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M65: DATABASE/SCHEMA/USER/VERSION session info functions.
    #[tokio::test]
    async fn session_info_functions_database_user_version() {
        let server = TestServer::start("session_info_fns").await;
        let mut client = server.connect().await;

        match client.query("SELECT DATABASE()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["rusql".to_string()]]);
            }
            other => panic!("expected DATABASE() rows, got {other:?}"),
        }
        match client.query("SELECT VERSION()").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][0].contains("8.0"),
                    "VERSION() should be MySQL 8.0-compatible, got {:?}",
                    rows[0][0]
                );
            }
            other => panic!("expected VERSION() rows, got {other:?}"),
        }
        match client.query("SELECT USER()").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][0].starts_with("root@"),
                    "USER() should be user@host, got {:?}",
                    rows[0][0]
                );
            }
            other => panic!("expected USER() rows, got {other:?}"),
        }

        assert!(matches!(
            client.query("CREATE DATABASE info_db").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.init_db("info_db").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT DATABASE()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["info_db".to_string()]]);
            }
            other => panic!("expected DATABASE() after USE, got {other:?}"),
        }
        match client.query("SELECT SCHEMA()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["info_db".to_string()]]);
            }
            other => panic!("expected SCHEMA() after USE, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M66: CASE / IF expressions in SELECT projections.
    #[tokio::test]
    async fn case_and_if_expressions() {
        let server = TestServer::start("case_if_expr").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE TABLE ci (id INT, name VARCHAR(8))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO ci VALUES (1, 'a')").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO ci VALUES (2, 'b')").await,
            QueryResponse::Ok { .. }
        ));

        match client
            .query("SELECT id, CASE WHEN id = 1 THEN 'one' ELSE 'other' END FROM ci ORDER BY id")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "one".to_string()],
                        vec!["2".to_string(), "other".to_string()],
                    ]
                );
            }
            other => panic!("expected CASE rows, got {other:?}"),
        }
        match client
            .query(
                "SELECT id, CASE name WHEN 'a' THEN 10 WHEN 'b' THEN 20 ELSE 0 END FROM ci ORDER BY id",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "10".to_string()],
                        vec!["2".to_string(), "20".to_string()],
                    ]
                );
            }
            other => panic!("expected simple CASE rows, got {other:?}"),
        }
        match client
            .query("SELECT id, IF(id = 1, 'yes', 'no') FROM ci ORDER BY id")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "yes".to_string()],
                        vec!["2".to_string(), "no".to_string()],
                    ]
                );
            }
            other => panic!("expected IF rows, got {other:?}"),
        }
        match client
            .query("SELECT CASE WHEN 1 = 1 THEN 'ok' ELSE 'no' END")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["ok".to_string()]]);
            }
            other => panic!("expected CASE without FROM, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M67: SELECT DISTINCT removes duplicate projected rows.
    #[tokio::test]
    async fn select_distinct() {
        let server = TestServer::start("select_distinct").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE TABLE d (id INT, tag VARCHAR(8))")
                .await,
            QueryResponse::Ok { .. }
        ));
        for sql in [
            "INSERT INTO d VALUES (1, 'a')",
            "INSERT INTO d VALUES (2, 'a')",
            "INSERT INTO d VALUES (3, 'b')",
        ] {
            assert!(matches!(client.query(sql).await, QueryResponse::Ok { .. }));
        }

        match client
            .query("SELECT DISTINCT tag FROM d ORDER BY tag")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["a".to_string()], vec!["b".to_string()]]);
            }
            other => panic!("expected DISTINCT rows, got {other:?}"),
        }

        match client
            .query("SELECT DISTINCT tag FROM d ORDER BY tag LIMIT 1")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["a".to_string()]]);
            }
            other => panic!("expected DISTINCT LIMIT rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M68: INSERT … SELECT copies query results; ODKU upserts on PRIMARY KEY.
    #[tokio::test]
    async fn insert_select_and_on_duplicate_key() {
        let server = TestServer::start("insert_select_odku").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE src (id INT PRIMARY KEY, name VARCHAR(16))",
            "CREATE TABLE dst (id INT PRIMARY KEY, name VARCHAR(16), cnt INT)",
            "INSERT INTO src VALUES (1, 'a')",
            "INSERT INTO src VALUES (2, 'b')",
            "INSERT INTO dst (id, name, cnt) SELECT id, name, 1 + 0 FROM src",
        ] {
            assert!(
                matches!(client.query(sql).await, QueryResponse::Ok { .. }),
                "failed: {sql}"
            );
        }

        match client
            .query("SELECT id, name, cnt FROM dst ORDER BY id")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "a".to_string(), "1".to_string()],
                        vec!["2".to_string(), "b".to_string(), "1".to_string()],
                    ]
                );
            }
            other => panic!("expected copied rows, got {other:?}"),
        }

        match client
            .query(
                "INSERT INTO dst VALUES (1, 'z', 9) ON DUPLICATE KEY UPDATE name = VALUES(name), cnt = cnt + 1",
            )
            .await
        {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 2),
            other => panic!("expected ODKU OK, got {other:?}"),
        }

        match client.query("SELECT name, cnt FROM dst WHERE id = 1").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["z".to_string(), "2".to_string()]]);
            }
            other => panic!("expected upserted row, got {other:?}"),
        }

        match client.query("INSERT INTO dst VALUES (1, 'nope', 0)").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1062),
            other => panic!("expected duplicate key error, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M69: non-recursive WITH CTE inlines as a derived table.
    #[tokio::test]
    async fn with_cte_select() {
        let server = TestServer::start("with_cte").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE t (id INT PRIMARY KEY, name VARCHAR(16))",
            "INSERT INTO t VALUES (1, 'a')",
            "INSERT INTO t VALUES (2, 'b')",
        ] {
            assert!(
                matches!(client.query(sql).await, QueryResponse::Ok { .. }),
                "failed: {sql}"
            );
        }

        match client
            .query("WITH c AS (SELECT id, name FROM t WHERE id = 2) SELECT id, name FROM c")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["2".to_string(), "b".to_string()]]);
            }
            other => panic!("expected CTE rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M70: ROW_NUMBER / RANK / DENSE_RANK over PARTITION BY + ORDER BY.
    #[tokio::test]
    async fn window_functions_select() {
        let server = TestServer::start("window_fn").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE t (id INT PRIMARY KEY, grp VARCHAR(8), score INT)",
            "INSERT INTO t VALUES (1, 'a', 10)",
            "INSERT INTO t VALUES (2, 'a', 10)",
            "INSERT INTO t VALUES (3, 'a', 20)",
            "INSERT INTO t VALUES (4, 'b', 5)",
        ] {
            assert!(
                matches!(client.query(sql).await, QueryResponse::Ok { .. }),
                "failed: {sql}"
            );
        }

        match client
            .query("SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS n FROM t ORDER BY id")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "1".to_string()],
                        vec!["2".to_string(), "2".to_string()],
                        vec!["3".to_string(), "3".to_string()],
                        vec!["4".to_string(), "4".to_string()],
                    ]
                );
            }
            other => panic!("expected row numbers, got {other:?}"),
        }

        match client
            .query(
                "SELECT grp, id, RANK() OVER (PARTITION BY grp ORDER BY score) AS r FROM t ORDER BY grp, id",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["a".to_string(), "1".to_string(), "1".to_string()],
                        vec!["a".to_string(), "2".to_string(), "1".to_string()],
                        vec!["a".to_string(), "3".to_string(), "3".to_string()],
                        vec!["b".to_string(), "4".to_string(), "1".to_string()],
                    ]
                );
            }
            other => panic!("expected ranks, got {other:?}"),
        }

        match client
            .query(
                "SELECT grp, id, DENSE_RANK() OVER (PARTITION BY grp ORDER BY score) AS d FROM t ORDER BY grp, id",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["a".to_string(), "1".to_string(), "1".to_string()],
                        vec!["a".to_string(), "2".to_string(), "1".to_string()],
                        vec!["a".to_string(), "3".to_string(), "2".to_string()],
                        vec!["b".to_string(), "4".to_string(), "1".to_string()],
                    ]
                );
            }
            other => panic!("expected dense ranks, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M71: COM_BINLOG_DUMP sends 0x00+event packets from the requested position.
    #[tokio::test]
    async fn binlog_dump_emits_per_event_packets() {
        let server = TestServer::start("binlog_dump").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE dump_t (id INT)",
            "BEGIN",
            "INSERT INTO dump_t VALUES (1)",
            "COMMIT",
        ] {
            let resp = client.query(sql).await;
            assert!(
                matches!(resp, QueryResponse::Ok { .. }),
                "failed: {sql} -> {resp:?}"
            );
        }

        let packets = client.binlog_dump(4).await;
        assert!(!packets.is_empty(), "expected at least FORMAT_DESCRIPTION");
        assert_eq!(packets[0][0], 0x00);
        // Event type is byte 4 of the event (payload[5] after 0x00).
        assert_eq!(
            packets[0][5], 15,
            "first event should be FORMAT_DESCRIPTION"
        );
        assert!(
            packets
                .iter()
                .any(|p| p.get(5) == Some(&EVENT_TYPE_TABLE_MAP)),
            "expected TABLE_MAP after COMMIT"
        );
        assert!(
            packets
                .iter()
                .any(|p| p.get(5) == Some(&EVENT_TYPE_WRITE_ROWS_V1)),
            "expected WRITE_ROWS after COMMIT"
        );
        assert!(
            packets.iter().all(|p| p.get(5) != Some(&2)),
            "INSERT should use row events, not QUERY_EVENT"
        );

        let mut reconstructed = BINLOG_MAGIC.to_vec();
        for packet in &packets {
            reconstructed.extend_from_slice(&packet[1..]);
        }
        let queries = extract_query_events(&reconstructed);
        assert!(
            queries.iter().any(|q| q.contains("INSERT INTO dump_t")),
            "dump should include committed INSERT, got {queries:?}"
        );

        let replica_dir = temp_data_dir("binlog_dump_replica");
        let _ = std::fs::remove_dir_all(&replica_dir);
        std::fs::create_dir_all(&replica_dir).unwrap();
        let dump_path = replica_dir.join("dump.bin");
        std::fs::write(&dump_path, &reconstructed).unwrap();
        let applied = apply_binlog_file(&dump_path, |_schema, sql| {
            assert!(sql.contains("INSERT") || sql.starts_with("/*"));
            Ok(())
        })
        .unwrap();
        assert!(applied >= 1);

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
        let _ = std::fs::remove_dir_all(&replica_dir);
    }

    /// M73: flags 0 keeps COM_BINLOG_DUMP open so a later COMMIT streams without reconnect.
    #[tokio::test]
    async fn binlog_dump_follow_receives_later_commit() {
        let server = TestServer::start("binlog_dump_follow").await;
        let mut sql = server.connect().await;
        assert!(
            matches!(
                sql.query("CREATE TABLE follow_t (id INT)").await,
                QueryResponse::Ok { .. }
            ),
            "CREATE TABLE follow_t"
        );

        let mut dump = server.connect().await;
        dump.send_binlog_dump(4, 0).await;

        let first =
            tokio::time::timeout(std::time::Duration::from_secs(2), dump.read_binlog_event())
                .await
                .expect("timed out waiting for FORMAT_DESCRIPTION");
        assert_eq!(first[0], 0x00);
        assert_eq!(first[5], 15, "first event should be FORMAT_DESCRIPTION");

        for text in ["BEGIN", "INSERT INTO follow_t VALUES (42)", "COMMIT"] {
            let resp = sql.query(text).await;
            assert!(
                matches!(resp, QueryResponse::Ok { .. }),
                "failed: {text} -> {resp:?}"
            );
        }

        let mut saw_table_map = false;
        let mut saw_write_rows = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline && !(saw_table_map && saw_write_rows) {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let packet = tokio::time::timeout(remaining, dump.read_binlog_event())
                .await
                .expect("timed out waiting for follow row events");
            assert_eq!(packet.first(), Some(&0x00));
            assert!(
                packet.len() > 19,
                "follow must send 0x00+event, not OK: {packet:?}"
            );
            match packet.get(5) {
                Some(&EVENT_TYPE_TABLE_MAP) => saw_table_map = true,
                Some(&EVENT_TYPE_WRITE_ROWS_V1) => saw_write_rows = true,
                _ => {}
            }
        }
        assert!(saw_table_map, "expected TABLE_MAP after live COMMIT");
        assert!(saw_write_rows, "expected WRITE_ROWS after live COMMIT");

        dump.quit().await;
        sql.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M74: committed UPDATE/DELETE dump as TABLE_MAP + UPDATE_ROWS / DELETE_ROWS.
    #[tokio::test]
    async fn binlog_dump_emits_update_and_delete_row_events() {
        let server = TestServer::start("binlog_dump_upd_del").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE dml_t (id INT, v INT)",
            "BEGIN",
            "INSERT INTO dml_t VALUES (1, 10)",
            "COMMIT",
            "BEGIN",
            "UPDATE dml_t SET v = 20 WHERE id = 1",
            "COMMIT",
            "BEGIN",
            "DELETE FROM dml_t WHERE id = 1",
            "COMMIT",
        ] {
            let resp = client.query(sql).await;
            assert!(
                matches!(resp, QueryResponse::Ok { .. }),
                "failed: {sql} -> {resp:?}"
            );
        }

        let packets = client.binlog_dump(4).await;
        assert!(
            packets
                .iter()
                .any(|p| p.get(5) == Some(&EVENT_TYPE_UPDATE_ROWS_V1)),
            "expected UPDATE_ROWS after COMMIT"
        );
        assert!(
            packets
                .iter()
                .any(|p| p.get(5) == Some(&EVENT_TYPE_DELETE_ROWS_V1)),
            "expected DELETE_ROWS after COMMIT"
        );
        assert!(
            packets.iter().all(|p| p.get(5) != Some(&2)),
            "UPDATE/DELETE should use row events, not QUERY_EVENT"
        );

        let mut reconstructed = BINLOG_MAGIC.to_vec();
        for packet in &packets {
            reconstructed.extend_from_slice(&packet[1..]);
        }
        let queries = extract_query_events(&reconstructed);
        assert!(
            queries.iter().any(|q| q.contains("UPDATE dml_t SET")),
            "dump should include committed UPDATE, got {queries:?}"
        );
        assert!(
            queries.iter().any(|q| q.contains("DELETE FROM dml_t")),
            "dump should include committed DELETE, got {queries:?}"
        );

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

//! Per-connection command loop after handshake.

use crate::audit::{redact_audit_command, AuditLog, DisconnectAudit, ErrWatch};
use crate::prepared::PreparedStatementStore;
use crate::replica_io::spawn_replica_dump;
use crate::slow_log::SlowQueryLog;
use crate::xa::{handle_xa_statement, xa_dml_blocked, XaConnectionState, XA_ERRNO};
use rusql_core::{
    parse_account_ddl, parse_password_policy_select, parse_password_policy_set,
    password_options_empty, Account, AccountDdl, AccountSpec, AlterUserAction, ConnectionRegistry,
    PasswordOptions, PasswordPolicyConfig, PasswordPolicySetParse, PrivilegeStore, ProgramStore,
    Session, UserLockRegistry, AUTH_PLUGIN_CACHING_SHA2,
};
use rusql_executor::{
    acquire_lock_tables, check_statement_privilege, clear_row_lock_registry,
    clear_table_lock_context, collect_insert_lock_pks, collect_update_lock_keys,
    exec_error_is_deadlock, execute, execute_flush_privileges, execute_grant,
    execute_replica_control, execute_revoke, execute_role_statement, execute_stored_program,
    global_read_only_assignment, install_slow_log_globals, lock_wait_timeout_for,
    plan_statement_table_lock_waits, read_slow_log_globals, release_lock_tables,
    resolve_lock_table_spec, run_due_events, session_tx_isolation, set_row_lock_registry,
    set_table_lock_context, show_slave_status, utc_now_stamp, wait_on_insert_pks,
    wait_on_registry_ids, wait_planned_table_locks, ExecError, GlobalReadOnlyAssignment,
    QueryResult,
};
use rusql_planner::plan;
use rusql_protocol::{
    authenticate_change_user, authenticate_handshake, binary_resultset_for_client, err_packet,
    err_packet_with_sqlstate, field_list_response, is_semi_sync_payload, mysql_type_from_sql_type,
    ok_packet_for_client, parse_command, parse_stmt_execute, password_contradicts_history,
    password_lifetime_elapsed, password_satisfies_policy, read_packet, semi_sync_slave_requested,
    stmt_eof_packet_for_client, stmt_field_definition, stmt_prepare_ok, text_resultset_for_client,
    write_packets, AuthLookupResult, ChangeUserRequest, ClientCommand, HandshakeConfig,
    HandshakeSession, HistorySample, ProtocolError, ValidatePasswordLevel,
    ValidatePasswordSettings, BINLOG_DUMP_NON_BLOCK, COM_QUIT,
    ER_CREDENTIALS_CONTRADICT_TO_HISTORY, ER_MUST_CHANGE_PASSWORD, ER_NOT_VALID_PASSWORD,
    ER_PLUGIN_IS_NOT_LOADED, MYSQL_TYPE_VAR_STRING, RPL_SEMI_SYNC_MASTER_PLUGIN,
    RPL_SEMI_SYNC_SLAVE_VAR,
};
use rusql_sql::{
    expand_com_query_batch, parse_for_session, parse_role_statement, role_statement_from_ast,
    role_statement_writes, try_parse_flush, try_parse_replica_control, try_parse_stored_program,
    try_parse_xa, FlushStatement, ReplicaControlStatement, RoleParse,
};
use rusql_storage::{
    binlog_row_image_assignment, dump_events_with_next_position, encode_heartbeat_log_event,
    master_heartbeat_period_from_dump, read_binlog_file, BinlogWriter, OverlayEngine,
    PersistentEngine, ReadOnlyEngine, ReplicaState, RowLockRegistry, TableLockRegistry,
    TransactionState,
};
use sqlparser::ast::Statement;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::{watch, RwLock as AsyncRwLock};
use tracing::{debug, warn};

struct BinlogHub {
    writer: Arc<AsyncRwLock<BinlogWriter>>,
    commits: watch::Sender<u64>,
}

fn new_transaction_state(session: &Session) -> TransactionState {
    TransactionState::with_isolation(session_tx_isolation(session))
}

fn begin_engine_statement(txn: &mut Option<TransactionState>) {
    if let Some(state) = txn.as_mut() {
        state.begin_statement();
    }
}

#[allow(clippy::too_many_arguments)]
async fn commit_open_transaction(
    session: &mut Session,
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
    binlog: &Arc<AsyncRwLock<BinlogWriter>>,
    binlog_commits: &watch::Sender<u64>,
    data_dir: &Path,
    row_locks: &Arc<RowLockRegistry>,
    txn: &mut Option<TransactionState>,
) -> Result<(), ProtocolError> {
    let Some(state) = txn.take() else {
        return Ok(());
    };
    row_locks.release_all(session.id);
    let records: Vec<_> = state.pending_records().to_vec();
    {
        let mut eng = engine.write().await;
        if let Err(e) = eng.commit_transaction(state.pending_records()) {
            return Err(ProtocolError::Message(e.to_string()));
        }
    }
    {
        let mut writer = binlog.write().await;
        if let Err(e) = writer.append_commit(data_dir, &session.database, &records) {
            return Err(ProtocolError::Message(e.to_string()));
        }
    }
    if !records.is_empty() {
        binlog_commits.send_modify(|n| *n = n.wrapping_add(1));
    }
    seed_session_catalog(session, engine).await;
    Ok(())
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

fn shared_replica(data_dir: &Path) -> Result<Arc<AsyncRwLock<ReplicaState>>, ProtocolError> {
    static REPLICAS: OnceLock<Mutex<HashMap<PathBuf, Arc<AsyncRwLock<ReplicaState>>>>> =
        OnceLock::new();
    let mut map = REPLICAS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let key = data_dir.to_path_buf();
    if let Some(existing) = map.get(&key) {
        return Ok(Arc::clone(existing));
    }
    let state = ReplicaState::load(data_dir).map_err(|e| ProtocolError::Message(e.to_string()))?;
    let arc = Arc::new(AsyncRwLock::new(state));
    map.insert(key, Arc::clone(&arc));
    Ok(arc)
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
            require_ssl: account.require_ssl,
        }));
    }
    if PrivilegeStore::is_superuser(username) {
        return Ok(Some(AuthLookupResult {
            password: String::new(),
            auth_plugin: AUTH_PLUGIN_CACHING_SHA2.to_string(),
            account_host: "%".into(),
            require_ssl: false,
        }));
    }
    Ok(None)
}

fn lookup_requires_ssl(lookup: &Option<AuthLookupResult>) -> bool {
    lookup.as_ref().is_some_and(|row| row.require_ssl)
}

/// Plaintext login for a `REQUIRE SSL` account is errno 1045 (`ER_ACCESS_DENIED_ERROR`).
async fn deny_plaintext_require_ssl<S>(
    stream: &mut S,
    seq: u8,
    lookup: &Option<AuthLookupResult>,
    using_tls: bool,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if using_tls || !lookup_requires_ssl(lookup) {
        return Ok(());
    }
    let err = err_packet(1045, &rusql_i18n::messages::protocol_access_denied());
    let _ = write_packets(stream, seq, &[err]).await;
    Err(ProtocolError::Message(
        rusql_i18n::messages::protocol_access_denied(),
    ))
}

/// `REQUIRE X509` / issuer / subject / cipher are not accepted (documented skip).
fn account_require_unsupported(sql: &str) -> bool {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    let upper = trimmed.to_ascii_uppercase();
    let user_ddl = upper.starts_with("CREATE USER") || upper.starts_with("ALTER USER");
    user_ddl && sql_has_keyword(trimmed, "REQUIRE")
}

fn sql_has_keyword(sql: &str, keyword: &str) -> bool {
    let mut in_quote: Option<char> = None;
    let mut token = String::new();
    let mut chars = sql.chars();
    let is_keyword = |token: &str| token.eq_ignore_ascii_case(keyword);
    while let Some(c) = chars.next() {
        if let Some(q) = in_quote {
            if c == '\\' {
                let _ = chars.next();
                continue;
            }
            if c == q {
                in_quote = None;
            }
            continue;
        }
        if c == '\'' || c == '"' || c == '`' {
            if is_keyword(&token) {
                return true;
            }
            token.clear();
            in_quote = Some(c);
            continue;
        }
        if c.is_ascii_alphanumeric() || c == '_' {
            token.push(c);
        } else {
            if is_keyword(&token) {
                return true;
            }
            token.clear();
        }
    }
    is_keyword(&token)
}

/// Run handshake then process COM_* commands until QUIT or disconnect.
#[allow(clippy::too_many_arguments)]
pub async fn serve_connection<S>(
    stream: S,
    config: &HandshakeConfig,
    connection_id: u32,
    engine: Arc<tokio::sync::RwLock<PersistentEngine>>,
    privileges: Arc<AsyncRwLock<PrivilegeStore>>,
    registry: Arc<ConnectionRegistry>,
    user_locks: Arc<UserLockRegistry>,
    row_locks: Arc<RowLockRegistry>,
    table_locks: Arc<TableLockRegistry>,
    data_dir: PathBuf,
    client_host: &str,
    tls: Option<crate::tls::ServerTlsAcceptor>,
    audit: Option<Arc<AuditLog>>,
    slow_log: Arc<SlowQueryLog>,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut conn_config = config.clone();
    if privileges.read().await.has_accounts() || conn_config.auth_credentials.is_some() {
        conn_config.ensure_caching_sha2_rsa();
    }
    let crate::tls::AcceptedConnection {
        mut io,
        handshake,
        response,
        auth_seq,
    } = crate::tls::accept_mysql_connection(stream, &conn_config, connection_id, tls).await?;
    let using_tls = io.is_tls();
    let stream = &mut io;
    let lookup =
        resolve_auth_lookup(&privileges, &conn_config, &response.username, client_host).await?;
    if conn_config.auth_credentials.is_none()
        && privileges.read().await.has_accounts()
        && lookup.is_none()
    {
        let err = err_packet(1045, &rusql_i18n::messages::protocol_access_denied());
        let _ = write_packets(stream, auth_seq, &[err]).await;
        return Err(ProtocolError::Message(
            rusql_i18n::messages::protocol_access_denied(),
        ));
    }
    deny_plaintext_require_ssl(stream, auth_seq, &lookup, using_tls).await?;
    let hs = authenticate_handshake(
        stream,
        &conn_config,
        &handshake,
        &response,
        lookup,
        auth_seq,
    )
    .await?;
    let conn_id = hs.connection_id as u64;
    registry.register(
        conn_id,
        &hs.username,
        client_host,
        hs.database.as_deref().unwrap_or("rusql"),
    );
    let audit_user = Arc::new(Mutex::new(hs.username.clone()));
    if let Some(log) = &audit {
        log.append(&hs.username, "Connect", 0);
    }
    let _disconnect = audit
        .clone()
        .map(|log| DisconnectAudit::arm(log, Arc::clone(&audit_user)));
    let result = run_command_loop(
        stream,
        hs,
        conn_config,
        engine,
        privileges,
        registry.clone(),
        user_locks.clone(),
        row_locks.clone(),
        table_locks.clone(),
        data_dir,
        client_host,
        using_tls,
        audit,
        audit_user,
        slow_log,
    )
    .await;
    user_locks.release_all(conn_id);
    row_locks.release_all(conn_id as u64);
    table_locks.release_all(conn_id as u64);
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
    user_locks: Arc<UserLockRegistry>,
    row_locks: Arc<RowLockRegistry>,
    table_locks: Arc<TableLockRegistry>,
    data_dir: PathBuf,
    client_host: &str,
    using_tls: bool,
    audit: Option<Arc<AuditLog>>,
    audit_user: Arc<Mutex<String>>,
    slow_log: Arc<SlowQueryLog>,
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
    session.user_locks = user_locks;
    let programs = Arc::new(AsyncRwLock::new(
        ProgramStore::load(&data_dir).unwrap_or_default(),
    ));
    let hub = shared_binlog(&data_dir)?;
    let binlog = hub.writer.clone();
    let binlog_commits = hub.commits.clone();
    {
        let writer = binlog.read().await;
        session.session_vars.insert(
            "binlog_row_image".to_string(),
            writer.row_image().as_str().to_string(),
        );
    }
    {
        let store = programs.read().await;
        store.seed_catalog(&mut session.catalog);
    }
    seed_session_catalog(&mut session, &engine).await;
    let mut txn: Option<TransactionState> = None;
    let mut xa: Option<XaConnectionState> = None;
    let mut stmts = PreparedStatementStore::new();
    let query_errno = Arc::new(AtomicU16::new(0));
    let mut io = ErrWatch::new(stream, Arc::clone(&query_errno));
    let stream = &mut io;

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
                if reject_must_change_password(stream, &privileges, &session.user, &session.host)
                    .await?
                {
                    continue;
                }
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
                if reject_must_change_password(stream, &privileges, &session.user, &session.host)
                    .await?
                {
                    continue;
                }
                registry.set_command(session.id, "FieldList", Some(&table));
                if let Err(e) =
                    handle_field_list(stream, &session, &table, hs.client_capabilities).await
                {
                    warn!(connection_id = hs.connection_id, error = %e, "field list failed");
                }
            }
            ClientCommand::ProcessInfo => {
                debug!(connection_id = hs.connection_id, "com_process_info");
                if reject_must_change_password(stream, &privileges, &session.user, &session.host)
                    .await?
                {
                    continue;
                }
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
                    &row_locks,
                    &table_locks,
                    using_tls,
                )
                .await
                {
                    Ok(updated) => {
                        hs = updated;
                        *audit_user.lock().unwrap_or_else(|e| e.into_inner()) =
                            session.user.clone();
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
                    &mut xa,
                    &row_locks,
                    &table_locks,
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
                let command = redact_audit_command(&sql);
                debug!(connection_id = hs.connection_id, sql = %command, "com_query");
                // M190: Questions counts this COM_QUERY packet, including SHOW STATUS.
                registry.increment_questions();
                registry.set_command(session.id, "Query", Some(&sql));
                query_errno.store(0, Ordering::Relaxed);
                install_slow_log_globals(&mut session, &slow_log.globals());
                let started = Instant::now();
                let batches = expand_com_query_batch(&sql, &mut session.client_delimiter);
                let mut protocol_error = false;
                if batches.is_empty() {
                    let ok =
                        ok_packet_for_client(0, session.last_insert_id, hs.client_capabilities);
                    if let Err(e) = write_packets(stream, 1, &[ok]).await {
                        warn!(connection_id = hs.connection_id, error = %e, "delimiter ok failed");
                        protocol_error = true;
                    }
                } else {
                    for batch in &batches {
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
                            &mut xa,
                            &row_locks,
                            &table_locks,
                            batch,
                            1,
                            None,
                            hs.client_capabilities,
                        )
                        .await
                        {
                            warn!(connection_id = hs.connection_id, error = %e, "query failed");
                            protocol_error = true;
                            break;
                        }
                    }
                }
                slow_log.absorb(&read_slow_log_globals(&session));
                if !protocol_error && !batches.is_empty() {
                    slow_log.maybe_append(&session, client_host, &sql, started.elapsed());
                }
                if protocol_error {
                    query_errno.store(0, Ordering::Relaxed);
                } else if let Some(log) = &audit {
                    let status = query_errno.swap(0, Ordering::Relaxed);
                    log.append(&session.user, &command, status);
                }
            }
            ClientCommand::BinlogDump {
                position,
                flags,
                heartbeat_period_ns,
                ..
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
                    heartbeat_period_ns,
                    session_requests_semi_sync(&session),
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
                if reject_must_change_password(stream, &privileges, &session.user, &session.host)
                    .await?
                {
                    continue;
                }
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
                if reject_must_change_password(stream, &privileges, &session.user, &session.host)
                    .await?
                {
                    continue;
                }
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
                    &mut xa,
                    &row_locks,
                    &table_locks,
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

pub(crate) async fn seed_session_catalog(
    session: &mut Session,
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
) {
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
    row_locks: &Arc<RowLockRegistry>,
    table_locks: &Arc<TableLockRegistry>,
    using_tls: bool,
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
    deny_plaintext_require_ssl(stream, 1, &lookup, using_tls).await?;
    let updated = authenticate_change_user(stream, config, hs, payload, lookup).await?;
    session.user = updated.username.clone();
    session.host = updated.account_host.clone();
    session.last_insert_id = 0;
    session.row_count = -1;
    session.found_rows = 0;
    session.sql_calc_found_rows = false;
    session.clear_session_vars();
    session.release_user_locks();
    row_locks.release_all(session.id);
    table_locks.release_all(session.id);
    if let Some(ref db) = updated.database {
        session.database = db.clone();
        seed_session_catalog(session, engine).await;
    }
    Ok(updated)
}

#[allow(clippy::too_many_arguments)]
async fn handle_reset_connection<S>(
    stream: &mut S,
    session: &mut Session,
    txn: &mut Option<TransactionState>,
    xa: &mut Option<XaConnectionState>,
    row_locks: &Arc<RowLockRegistry>,
    table_locks: &Arc<TableLockRegistry>,
    stmts: &mut PreparedStatementStore,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    *txn = None;
    *xa = None;
    *stmts = PreparedStatementStore::new();
    session.last_insert_id = 0;
    session.row_count = -1;
    session.found_rows = 0;
    session.sql_calc_found_rows = false;
    session.clear_session_vars();
    session.release_user_locks();
    row_locks.release_all(session.id);
    table_locks.release_all(session.id);
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

fn session_requests_semi_sync(session: &Session) -> bool {
    session
        .user_vars
        .get(RPL_SEMI_SYNC_SLAVE_VAR)
        .is_some_and(|value| semi_sync_slave_requested(value))
}

fn semi_sync_unsupported_err() -> Vec<u8> {
    err_packet(
        ER_PLUGIN_IS_NOT_LOADED,
        &rusql_i18n::messages::protocol_plugin_is_not_loaded(RPL_SEMI_SYNC_MASTER_PLUGIN),
    )
}

/// Returns `true` when the dump connection should disconnect (follow ended).
#[allow(clippy::too_many_arguments)]
async fn handle_binlog_dump<S>(
    stream: &mut S,
    binlog: &Arc<AsyncRwLock<BinlogWriter>>,
    binlog_commits: &watch::Sender<u64>,
    mut position: u32,
    flags: u16,
    heartbeat_period_ns: Option<u64>,
    semi_sync: bool,
    client_caps: u32,
) -> Result<bool, ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // M178: do not start a dump that looks like semi-sync success. No ACK wait.
    if semi_sync {
        debug!("rejecting semi-sync dump; rpl_semi_sync_master is not loaded");
        write_packets(stream, 1, &[semi_sync_unsupported_err()]).await?;
        return Ok(false);
    }
    let non_block = flags & BINLOG_DUMP_NON_BLOCK != 0;
    let mut rx = binlog_commits.subscribe();
    rx.borrow_and_update();
    let mut seq = 1u8;
    let heartbeat_period = if non_block {
        None
    } else {
        Some(master_heartbeat_period_from_dump(heartbeat_period_ns))
    };
    let mut heartbeat = heartbeat_period.map(|period| {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval
    });
    if let Some(ref mut interval) = heartbeat {
        interval.tick().await;
    }
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
        let interval = heartbeat
            .as_mut()
            .expect("follow dump always configures heartbeat interval");
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
                    Ok((_, payload)) if is_semi_sync_payload(&payload) => {
                        debug!("rejecting semi-sync ACK; rpl_semi_sync_master is not loaded");
                        if write_packets(stream, seq, &[semi_sync_unsupported_err()])
                            .await
                            .is_err()
                        {
                            return Ok(true);
                        }
                        return Ok(false);
                    }
                    Ok(_) => {}
                    Err(_) => return Ok(true),
                }
            }
            _ = interval.tick() => {
                let (server_id, log_pos) = {
                    let writer = binlog.read().await;
                    (writer.server_id(), writer.current_position())
                };
                let event = encode_heartbeat_log_event(server_id, log_pos);
                let mut packet = Vec::with_capacity(1 + event.len());
                packet.push(0x00);
                packet.extend_from_slice(&event);
                if write_packets(stream, seq, &[packet]).await.is_err() {
                    return Ok(true);
                }
                seq = seq.wrapping_add(1);
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
    xa: &mut Option<XaConnectionState>,
    row_locks: &Arc<RowLockRegistry>,
    table_locks: &Arc<TableLockRegistry>,
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
        xa,
        row_locks,
        table_locks,
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
    if upper == "SHOW MASTER STATUS" || upper == "SHOW BINARY LOG STATUS" {
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
    replica: &Arc<AsyncRwLock<ReplicaState>>,
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
    let pos = format!("{}", writer.current_position());
    drop(writer);
    let executed = gtid.executed_gtid_set();
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
                executed.clone(),
            ]],
        ),
        ReplicationStatusKind::Slave => {
            let state = replica.read().await;
            match show_slave_status(&state, &executed) {
                QueryResult::Rows { columns, rows } => (columns, rows),
                QueryResult::Ok { .. } => (Vec::new(), Vec::new()),
            }
        }
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
    xa: &mut Option<XaConnectionState>,
    row_locks: &Arc<RowLockRegistry>,
    table_locks: &Arc<TableLockRegistry>,
    sql: &str,
    seq_start: u8,
    binary_column_types: Option<&[u8]>,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if let Some(xa_parse) = try_parse_xa(sql) {
        return match xa_parse {
            Ok(stmt) => {
                let commit_like = matches!(
                    stmt,
                    rusql_sql::XaStatement::Commit { .. } | rusql_sql::XaStatement::Prepare { .. }
                );
                if commit_like && reject_read_only_write(stream, engine, &session.user).await? {
                    return Ok(());
                }
                handle_xa_statement(
                    stream,
                    session,
                    engine,
                    binlog,
                    binlog_commits,
                    data_dir,
                    txn,
                    xa,
                    row_locks,
                    stmt,
                    seq_start,
                    client_caps,
                )
                .await?;
                if commit_like {
                    seed_session_catalog(session, engine).await;
                }
                Ok(())
            }
            Err(e) => {
                let err = err_packet(1064, &e.to_string());
                write_packets(stream, seq_start, &[err]).await?;
                Ok(())
            }
        };
    }

    if xa_dml_blocked(xa) {
        let err = err_packet(XA_ERRNO, &rusql_i18n::messages::sql_xa_rmfail_idle());
        write_packets(stream, seq_start, &[err]).await?;
        return Ok(());
    }

    {
        let store = privileges.read().await;
        let mut prog_store = programs.write().await;
        let mut eng = engine.write().await;
        let now = utc_now_stamp();
        let ran = match txn {
            Some(ref mut t) => {
                let mut overlay = OverlayEngine::new(&eng, t);
                run_due_events(&mut overlay, session, &mut prog_store, Some(&store), &now)
            }
            None => run_due_events(&mut *eng, session, &mut prog_store, Some(&store), &now),
        };
        if let Err(e) = ran {
            warn!(error = %e, "event scheduler skipped due events");
        }
        if let Err(e) = prog_store.save(data_dir) {
            warn!(error = %e, "event scheduler failed to persist catalog");
        }
    }

    if let Some(parsed) = parse_password_policy_set(sql) {
        if reject_read_only_write(stream, engine, &session.user).await? {
            return Ok(());
        }
        return handle_password_policy_set(
            stream,
            session,
            privileges,
            data_dir,
            parsed,
            client_caps,
        )
        .await;
    }

    if let Some(read) = parse_password_policy_select(sql) {
        if session_must_change(privileges, &session.user, &session.host).await {
            let err = err_packet(
                ER_MUST_CHANGE_PASSWORD,
                &rusql_i18n::messages::sql_must_change_password(),
            );
            write_packets(stream, seq_start, &[err]).await?;
            return Ok(());
        }
        let value = {
            let store = privileges.read().await;
            store.password_policy_text(&read.field)
        };
        let payloads = text_resultset_for_client(&[read.column], &[vec![value]], client_caps);
        write_packets(stream, seq_start, &payloads).await?;
        return Ok(());
    }

    if let Some(ddl) = parse_account_ddl(sql) {
        if reject_read_only_write(stream, engine, &session.user).await? {
            return Ok(());
        }
        if session_must_change(privileges, &session.user, &session.host).await
            && !ddl_is_self_password_change(&ddl, &session.user, &session.host)
        {
            let err = err_packet(
                ER_MUST_CHANGE_PASSWORD,
                &rusql_i18n::messages::sql_must_change_password(),
            );
            write_packets(stream, seq_start, &[err]).await?;
            return Ok(());
        }
        return handle_account_ddl(stream, session, privileges, data_dir, ddl, client_caps).await;
    }

    if account_require_unsupported(sql) {
        let err = err_packet(1064, &rusql_i18n::messages::sql_parse_error("REQUIRE"));
        write_packets(stream, seq_start, &[err]).await?;
        return Ok(());
    }

    if session_must_change(privileges, &session.user, &session.host).await {
        let err = err_packet(
            ER_MUST_CHANGE_PASSWORD,
            &rusql_i18n::messages::sql_must_change_password(),
        );
        write_packets(stream, seq_start, &[err]).await?;
        return Ok(());
    }

    if let Some(parsed) = try_parse_flush(sql) {
        return handle_flush_statement(
            stream,
            session,
            privileges,
            data_dir,
            parsed,
            client_caps,
            seq_start,
        )
        .await;
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
        let replica = shared_replica(data_dir)?;
        return write_replication_status(stream, status, binlog, &replica, client_caps).await;
    }

    if let Some(replica_parse) = try_parse_replica_control(sql) {
        return match replica_parse {
            Ok(stmt) => {
                let replica = shared_replica(data_dir)?;
                let start = matches!(stmt, ReplicaControlStatement::StartSlave);
                let result = {
                    let mut state = replica.write().await;
                    execute_replica_control(&mut state, stmt)
                };
                match result {
                    Ok(QueryResult::Ok { rows_affected }) => {
                        if start {
                            let generation = replica.read().await.status.generation;
                            spawn_replica_dump(
                                replica.clone(),
                                engine.clone(),
                                binlog.clone(),
                                generation,
                            );
                        }
                        let ok = ok_packet_for_client(
                            rows_affected,
                            session.last_insert_id,
                            client_caps,
                        );
                        write_packets(stream, seq_start, &[ok]).await?;
                    }
                    Ok(QueryResult::Rows { columns, rows }) => {
                        let payloads = text_resultset_for_client(&columns, &rows, client_caps);
                        write_packets(stream, seq_start, &payloads).await?;
                    }
                    Err(e) => write_exec_error(stream, e).await?,
                }
                Ok(())
            }
            Err(e) => {
                let err = err_packet(1064, &e.to_string());
                write_packets(stream, seq_start, &[err]).await?;
                Ok(())
            }
        };
    }

    if let Some(stmt) = try_parse_stored_program(sql) {
        if reject_read_only_write(stream, engine, &session.user).await? {
            return Ok(());
        }
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
                match r {
                    QueryResult::Ok { rows_affected } => {
                        let ok = ok_packet_for_client(
                            rows_affected,
                            session.last_insert_id,
                            client_caps,
                        );
                        write_packets(stream, seq_start, &[ok]).await?;
                    }
                    QueryResult::Rows { columns, rows } => {
                        let payloads = text_resultset_for_client(&columns, &rows, client_caps);
                        write_packets(stream, seq_start, &payloads).await?;
                    }
                }
            }
            Err(e) => write_exec_error(stream, e).await?,
        }
        return Ok(());
    }

    if let RoleParse::Statement(role_stmt) = parse_role_statement(sql) {
        if role_statement_writes(&role_stmt)
            && reject_read_only_write(stream, engine, &session.user).await?
        {
            return Ok(());
        }
        let result = {
            let mut store = privileges.write().await;
            let result = execute_role_statement(&mut store, session, &role_stmt);
            if result.is_ok() && role_statement_writes(&role_stmt) {
                store.save(data_dir).map_err(|e| {
                    ProtocolError::Message(format!("failed to save privileges: {e}"))
                })?;
            }
            result
        };
        match result {
            Ok(QueryResult::Ok { rows_affected }) => {
                let ok = ok_packet_for_client(rows_affected, session.last_insert_id, client_caps);
                write_packets(stream, seq_start, &[ok]).await?;
            }
            Ok(QueryResult::Rows { columns, rows }) => {
                let payloads = text_resultset_for_client(&columns, &rows, client_caps);
                write_packets(stream, seq_start, &payloads).await?;
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
        match binlog_row_image_assignment(&stmt) {
            Err(e) => {
                let err = err_packet(e.code, &e.message);
                write_packets(stream, 1, &[err]).await?;
                return Ok(());
            }
            Ok(Some(image)) => {
                let mut writer = binlog.write().await;
                if let Err(e) = writer.set_row_image(data_dir, image) {
                    let err = err_packet(1105, &e.to_string());
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                drop(writer);
                session
                    .session_vars
                    .insert("binlog_row_image".to_string(), image.as_str().to_string());
                all_results.push(QueryResult::Ok { rows_affected: 0 });
                continue;
            }
            Ok(None) => {}
        }
        match global_read_only_assignment(&stmt) {
            Err(e) => {
                write_exec_error(stream, e).await?;
                return Ok(());
            }
            Ok(Some(assign)) => {
                let mut eng = engine.write().await;
                let saved = match assign {
                    GlobalReadOnlyAssignment::ReadOnly(on) => eng.set_replica_read_only(on),
                    GlobalReadOnlyAssignment::SuperReadOnly(on) => {
                        eng.set_replica_super_read_only(on)
                    }
                };
                drop(eng);
                if saved.is_err() {
                    let err = err_packet(
                        1105,
                        &rusql_i18n::messages::sql_replica_role_persist_failed(),
                    );
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                all_results.push(QueryResult::Ok { rows_affected: 0 });
                continue;
            }
            Ok(None) => {}
        }
        match stmt {
            Statement::StartTransaction { .. } => {
                if xa.is_some() {
                    let err = err_packet(XA_ERRNO, &rusql_i18n::messages::sql_xa_rmfail_active());
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                if txn.is_some() {
                    let err = err_packet(1105, "transaction already active");
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                *txn = Some(new_transaction_state(session));
                all_results.push(QueryResult::Ok { rows_affected: 0 });
            }
            Statement::Commit { .. } => {
                if xa.is_some() {
                    let err = err_packet(XA_ERRNO, &rusql_i18n::messages::sql_xa_rmfail_active());
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                let pending = txn
                    .as_ref()
                    .is_some_and(|state| !state.pending_records().is_empty());
                if pending && reject_read_only_write(stream, engine, &session.user).await? {
                    return Ok(());
                }
                let Some(state) = txn.take() else {
                    let err = err_packet(1105, "no active transaction");
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                };
                row_locks.release_all(session.id);
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
            Statement::Rollback { savepoint, .. } => {
                if xa.is_some() && savepoint.is_none() {
                    let err = err_packet(XA_ERRNO, &rusql_i18n::messages::sql_xa_rmfail_active());
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                if let Some(name) = savepoint {
                    let Some(state) = txn.as_mut() else {
                        let err = err_packet(
                            1305,
                            &rusql_i18n::messages::sql_savepoint_does_not_exist(&name.value),
                        );
                        write_packets(stream, 1, &[err]).await?;
                        return Ok(());
                    };
                    if !state.rollback_to_savepoint(&name.value) {
                        let err = err_packet(
                            1305,
                            &rusql_i18n::messages::sql_savepoint_does_not_exist(&name.value),
                        );
                        write_packets(stream, 1, &[err]).await?;
                        return Ok(());
                    }
                    all_results.push(QueryResult::Ok { rows_affected: 0 });
                } else {
                    if txn.is_none() {
                        let err = err_packet(1105, "no active transaction");
                        write_packets(stream, 1, &[err]).await?;
                        return Ok(());
                    }
                    row_locks.release_all(session.id);
                    let _ = txn.take();
                    all_results.push(QueryResult::Ok { rows_affected: 0 });
                }
            }
            Statement::Savepoint { name } => {
                if txn.is_none() {
                    *txn = Some(new_transaction_state(session));
                }
                txn.as_mut()
                    .expect("savepoint txn")
                    .create_savepoint(&name.value);
                all_results.push(QueryResult::Ok { rows_affected: 0 });
            }
            Statement::ReleaseSavepoint { name } => {
                let Some(state) = txn.as_mut() else {
                    let err = err_packet(
                        1305,
                        &rusql_i18n::messages::sql_savepoint_does_not_exist(&name.value),
                    );
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                };
                if !state.release_savepoint(&name.value) {
                    let err = err_packet(
                        1305,
                        &rusql_i18n::messages::sql_savepoint_does_not_exist(&name.value),
                    );
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                all_results.push(QueryResult::Ok { rows_affected: 0 });
            }
            Statement::LockTables { tables } => {
                if reject_read_only_write(stream, engine, &session.user).await? {
                    return Ok(());
                }
                if xa.is_some() {
                    let err = err_packet(XA_ERRNO, &rusql_i18n::messages::sql_xa_rmfail_active());
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                if let Err(e) = commit_open_transaction(
                    session,
                    engine,
                    binlog,
                    binlog_commits,
                    data_dir,
                    row_locks,
                    txn,
                )
                .await
                {
                    let err = err_packet(1105, &e.to_string());
                    write_packets(stream, 1, &[err]).await?;
                    return Ok(());
                }
                let spec = match resolve_lock_table_spec(session, &tables) {
                    Ok(s) => s,
                    Err(e) => {
                        write_exec_error(stream, e).await?;
                        return Ok(());
                    }
                };
                let timeout = lock_wait_timeout_for(session);
                let reg = table_locks.clone();
                let sid = session.id;
                let wait = tokio::task::spawn_blocking(move || {
                    acquire_lock_tables(&reg, sid, &spec, timeout)
                })
                .await;
                match wait {
                    Ok(Ok(())) => all_results.push(QueryResult::Ok { rows_affected: 0 }),
                    Ok(Err(e)) => {
                        write_exec_error(stream, e).await?;
                        return Ok(());
                    }
                    Err(e) => {
                        let err = err_packet(1105, &format!("table lock wait failed: {e}"));
                        write_packets(stream, 1, &[err]).await?;
                        return Ok(());
                    }
                }
            }
            Statement::UnlockTables => {
                release_lock_tables(table_locks, session.id);
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
                if let Some(role_stmt) = role_statement_from_ast(&other) {
                    if role_statement_writes(&role_stmt)
                        && reject_read_only_write(stream, engine, &session.user).await?
                    {
                        return Ok(());
                    }
                    let result = {
                        let mut store = privileges.write().await;
                        let result = execute_role_statement(&mut store, session, &role_stmt);
                        if result.is_ok() && role_statement_writes(&role_stmt) {
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
                    continue;
                }
                if !is_read_only_statement(&other)
                    && reject_read_only_write(stream, engine, &session.user).await?
                {
                    return Ok(());
                }
                if let Err(e) = {
                    let store = privileges.read().await;
                    check_statement_privilege(&store, session, &other)
                } {
                    write_exec_error(stream, e).await?;
                    return Ok(());
                }
                let held = if table_locks.in_lock_tables_mode(session.id) {
                    table_locks.session_locks(session.id)
                } else {
                    None
                };
                set_table_lock_context(table_locks.clone(), session.id, held.clone());
                let timeout = lock_wait_timeout_for(session);
                let lock_plan =
                    match plan_statement_table_lock_waits(session, &other, held.as_ref()) {
                        Ok(p) => p,
                        Err(e) => {
                            clear_table_lock_context();
                            write_exec_error(stream, e).await?;
                            return Ok(());
                        }
                    };
                let reg = table_locks.clone();
                let sid = session.id;
                let wait = tokio::task::spawn_blocking(move || {
                    wait_planned_table_locks(&reg, sid, &lock_plan, timeout)
                })
                .await;
                match wait {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        clear_table_lock_context();
                        write_exec_error(stream, e).await?;
                        return Ok(());
                    }
                    Err(e) => {
                        clear_table_lock_context();
                        let err = err_packet(1105, &format!("table lock wait failed: {e}"));
                        write_packets(stream, 1, &[err]).await?;
                        return Ok(());
                    }
                }
                begin_engine_statement(txn);
                let read_only = is_read_only_statement(&other);
                let plans = plan(session, vec![other]);
                {
                    let eng = engine.read().await;
                    let keys = match collect_update_lock_keys(&*eng, session, &plans) {
                        Ok(k) => k,
                        Err(e) => {
                            write_exec_error(stream, e).await?;
                            return Ok(());
                        }
                    };
                    drop(eng);
                    if !keys.is_empty() {
                        let reg = row_locks.clone();
                        let sid = session.id;
                        let timeout = lock_wait_timeout_for(session);
                        let wait = tokio::task::spawn_blocking(move || {
                            wait_on_registry_ids(&reg, sid, timeout, &keys)
                        })
                        .await;
                        match wait {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => {
                                if exec_error_is_deadlock(&e) {
                                    row_locks.release_all(session.id);
                                    txn.take();
                                }
                                write_exec_error(stream, e).await?;
                                return Ok(());
                            }
                            Err(e) => {
                                let err = err_packet(1105, &format!("row lock wait failed: {e}"));
                                write_packets(stream, 1, &[err]).await?;
                                return Ok(());
                            }
                        }
                    }
                    let insert_pks = match collect_insert_lock_pks(session, &plans) {
                        Ok(p) => p,
                        Err(e) => {
                            write_exec_error(stream, e).await?;
                            return Ok(());
                        }
                    };
                    if !insert_pks.is_empty() {
                        let reg = row_locks.clone();
                        let sid = session.id;
                        let timeout = lock_wait_timeout_for(session);
                        let wait = tokio::task::spawn_blocking(move || {
                            wait_on_insert_pks(&reg, sid, timeout, &insert_pks)
                        })
                        .await;
                        match wait {
                            Ok(Ok(())) => {}
                            Ok(Err(e)) => {
                                if exec_error_is_deadlock(&e) {
                                    row_locks.release_all(session.id);
                                    txn.take();
                                }
                                write_exec_error(stream, e).await?;
                                return Ok(());
                            }
                            Err(e) => {
                                let err = err_packet(1105, &format!("row lock wait failed: {e}"));
                                write_packets(stream, 1, &[err]).await?;
                                return Ok(());
                            }
                        }
                    }
                }
                set_row_lock_registry(row_locks.clone());
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
                        match txn {
                            Some(ref mut t) => {
                                let eng = engine.read().await;
                                let mut overlay = OverlayEngine::new(&eng, t);
                                execute(&mut overlay, session, &plans, Some(&store))
                            }
                            None => {
                                let mut eng = engine.write().await;
                                execute(&mut *eng, session, &plans, Some(&store))
                            }
                        }
                    }
                };
                clear_row_lock_registry();
                clear_table_lock_context();
                match results {
                    Ok(r) => all_results.extend(r),
                    Err(e) => {
                        if exec_error_is_deadlock(&e) {
                            row_locks.release_all(session.id);
                            txn.take();
                        }
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
    if txn.is_none() && xa.is_none() {
        row_locks.release_all(session.id);
    }
    Ok(())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn account_must_change(store: &PrivilegeStore, user: &str, host: &str) -> bool {
    let Some(status) = store.password_status(user, host) else {
        return false;
    };
    status.expired
        || password_lifetime_elapsed(status.changed_unix, status.lifetime_days, unix_now())
}

async fn session_must_change(
    privileges: &Arc<AsyncRwLock<PrivilegeStore>>,
    user: &str,
    host: &str,
) -> bool {
    let store = privileges.read().await;
    account_must_change(&store, user, host)
}

async fn reject_must_change_password<S>(
    stream: &mut S,
    privileges: &Arc<AsyncRwLock<PrivilegeStore>>,
    user: &str,
    host: &str,
) -> Result<bool, ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if !session_must_change(privileges, user, host).await {
        return Ok(false);
    }
    let err = err_packet(
        ER_MUST_CHANGE_PASSWORD,
        &rusql_i18n::messages::sql_must_change_password(),
    );
    write_packets(stream, 1, &[err]).await?;
    Ok(true)
}

fn validate_settings(cfg: &PasswordPolicyConfig) -> ValidatePasswordSettings {
    let level = match cfg.validate_policy.to_ascii_uppercase().as_str() {
        "LOW" => ValidatePasswordLevel::Low,
        "MEDIUM" => ValidatePasswordLevel::Medium,
        "STRONG" => ValidatePasswordLevel::Strong,
        _ => ValidatePasswordLevel::Off,
    };
    ValidatePasswordSettings {
        level,
        length: cfg.validate_length,
    }
}

fn same_login(account: &Account, user: &str, host: &str) -> bool {
    account.user.eq_ignore_ascii_case(user) && account.host == host
}

fn ddl_is_self_password_change(ddl: &AccountDdl, user: &str, host: &str) -> bool {
    match ddl {
        AccountDdl::SetPassword { account, .. } => match account {
            None => true,
            Some(account) => same_login(account, user, host),
        },
        AccountDdl::AlterUser { accounts, action } => {
            let sets_password = matches!(action, AlterUserAction::Identified { .. });
            sets_password
                && accounts.len() == 1
                && match &accounts[0] {
                    AccountSpec::Current => true,
                    AccountSpec::Named(account) => same_login(account, user, host),
                }
        }
        AccountDdl::CreateUser { .. } | AccountDdl::DropUser { .. } => false,
    }
}

fn resolve_account_spec(spec: &AccountSpec, session: &Session) -> Account {
    match spec {
        AccountSpec::Current => Account::new(&session.user, &session.host),
        AccountSpec::Named(account) => account.clone(),
    }
}

fn alter_has_admin_options(action: &AlterUserAction) -> bool {
    match action {
        AlterUserAction::Options(_) => true,
        AlterUserAction::Identified { options, .. } => !password_options_empty(options),
    }
}

async fn write_mysql<S>(stream: &mut S, code: u16, message: &str) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let err = err_packet(code, message);
    write_packets(stream, 1, &[err]).await?;
    Ok(())
}

async fn handle_flush_statement<S>(
    stream: &mut S,
    session: &Session,
    privileges: &Arc<AsyncRwLock<PrivilegeStore>>,
    data_dir: &Path,
    parsed: Result<FlushStatement, rusql_sql::SqlError>,
    client_caps: u32,
    seq_start: u8,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let stmt = match parsed {
        Ok(stmt) => stmt,
        Err(error) => {
            return write_mysql(stream, 1064, &error.to_string()).await;
        }
    };
    if !PrivilegeStore::is_superuser(&session.user) {
        return write_mysql(
            stream,
            1227,
            &rusql_i18n::messages::sql_reload_privilege_required(&session.user, &session.host),
        )
        .await;
    }
    match stmt {
        FlushStatement::Unsupported { option } => {
            write_mysql(
                stream,
                1235,
                &rusql_i18n::messages::sql_flush_not_supported(&option),
            )
            .await
        }
        FlushStatement::Privileges => {
            let result = {
                let mut store = privileges.write().await;
                execute_flush_privileges(&mut store, session, data_dir)
            };
            match result {
                Ok(QueryResult::Ok { rows_affected }) => {
                    let ok =
                        ok_packet_for_client(rows_affected, session.last_insert_id, client_caps);
                    write_packets(stream, seq_start, &[ok]).await?;
                    Ok(())
                }
                Ok(QueryResult::Rows { columns, rows }) => {
                    let payloads = text_resultset_for_client(&columns, &rows, client_caps);
                    write_packets(stream, seq_start, &payloads).await?;
                    Ok(())
                }
                Err(error) => write_exec_error(stream, error).await,
            }
        }
    }
}

async fn handle_password_policy_set<S>(
    stream: &mut S,
    session: &Session,
    privileges: &Arc<AsyncRwLock<PrivilegeStore>>,
    data_dir: &Path,
    parsed: PasswordPolicySetParse,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if session_must_change(privileges, &session.user, &session.host).await {
        return write_mysql(
            stream,
            ER_MUST_CHANGE_PASSWORD,
            &rusql_i18n::messages::sql_must_change_password(),
        )
        .await;
    }
    if !PrivilegeStore::is_superuser(&session.user) {
        return write_mysql(
            stream,
            1227,
            &rusql_i18n::messages::account_admin_required(&session.user, &session.host),
        )
        .await;
    }
    match parsed {
        PasswordPolicySetParse::NeedsGlobal(name) => {
            write_mysql(
                stream,
                1229,
                &rusql_i18n::messages::sql_global_variable_requires_set_global(&name),
            )
            .await
        }
        PasswordPolicySetParse::BadValue { name, value } => {
            write_mysql(
                stream,
                1231,
                &rusql_i18n::messages::sql_wrong_value_for_var(&name, &value),
            )
            .await
        }
        PasswordPolicySetParse::Assign(assign) => {
            let mut store = privileges.write().await;
            store.apply_password_policy_assign(assign);
            store
                .save(data_dir)
                .map_err(|e| ProtocolError::Message(format!("failed to save privileges: {e}")))?;
            let ok = ok_packet_for_client(0, 0, client_caps);
            write_packets(stream, 1, &[ok]).await?;
            Ok(())
        }
    }
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
    let superuser = PrivilegeStore::is_superuser(&session.user);
    let allowed = match &ddl {
        AccountDdl::CreateUser { .. } | AccountDdl::DropUser { .. } => superuser,
        AccountDdl::AlterUser { action, .. } => {
            let self_change = ddl_is_self_password_change(&ddl, &session.user, &session.host);
            superuser || (self_change && !alter_has_admin_options(action))
        }
        AccountDdl::SetPassword { .. } => {
            superuser || ddl_is_self_password_change(&ddl, &session.user, &session.host)
        }
    };
    if !allowed {
        return write_mysql(
            stream,
            1227,
            &rusql_i18n::messages::account_admin_required(&session.user, &session.host),
        )
        .await;
    }

    let mut store = privileges.write().await;
    let now = unix_now();
    let settings = validate_settings(store.password_policy());
    if let Err(code_message) = apply_account_ddl(&mut store, session, &ddl, settings, now) {
        return write_mysql(stream, code_message.0, &code_message.1).await;
    }
    store
        .save(data_dir)
        .map_err(|e| ProtocolError::Message(format!("failed to save privileges: {e}")))?;
    let ok = ok_packet_for_client(0, 0, client_caps);
    write_packets(stream, 1, &[ok]).await?;
    Ok(())
}

fn apply_account_ddl(
    store: &mut PrivilegeStore,
    session: &Session,
    ddl: &AccountDdl,
    settings: ValidatePasswordSettings,
    now: u64,
) -> Result<(), (u16, String)> {
    match ddl {
        AccountDdl::CreateUser {
            accounts,
            auth_plugin,
            password,
            if_not_exists,
            options,
        } => {
            for account in accounts {
                if !password_satisfies_policy(password, &account.user, settings) {
                    return Err((
                        ER_NOT_VALID_PASSWORD,
                        rusql_i18n::messages::sql_password_not_valid(),
                    ));
                }
            }
            for account in accounts {
                if let Err(message) = store.create_user_with_options(
                    account,
                    password.clone(),
                    auth_plugin.clone(),
                    *if_not_exists,
                    options,
                    now,
                ) {
                    return Err((1105, message));
                }
            }
            Ok(())
        }
        AccountDdl::DropUser {
            accounts,
            if_exists,
        } => {
            for account in accounts {
                if let Err(message) = store.drop_user(account, *if_exists) {
                    return Err((1105, message));
                }
            }
            Ok(())
        }
        AccountDdl::AlterUser { accounts, action } => {
            let targets: Vec<Account> = accounts
                .iter()
                .map(|spec| resolve_account_spec(spec, session))
                .collect();
            match action {
                AlterUserAction::Options(options) => {
                    for account in &targets {
                        if store
                            .password_status(&account.user, &account.host)
                            .is_none()
                        {
                            return Err((
                                1396,
                                rusql_i18n::messages::sql_cannot_user(
                                    "ALTER USER",
                                    &account.display(),
                                ),
                            ));
                        }
                    }
                    for account in &targets {
                        store.apply_password_options_only(account, options);
                    }
                }
                AlterUserAction::Identified {
                    auth_plugin,
                    password,
                    options,
                } => {
                    check_new_passwords(
                        store,
                        &targets,
                        password,
                        options,
                        settings,
                        now,
                        "ALTER USER",
                    )?;
                    for account in &targets {
                        store.commit_new_password(
                            account,
                            password.clone(),
                            auth_plugin.clone(),
                            options,
                            now,
                        );
                    }
                }
            }
            Ok(())
        }
        AccountDdl::SetPassword { account, password } => {
            let target = account
                .clone()
                .unwrap_or_else(|| Account::new(&session.user, &session.host));
            let options = PasswordOptions::default();
            check_new_passwords(
                store,
                std::slice::from_ref(&target),
                password,
                &options,
                settings,
                now,
                "SET PASSWORD",
            )?;
            store.commit_new_password(&target, password.clone(), None, &options, now);
            Ok(())
        }
    }
}

fn check_new_passwords(
    store: &PrivilegeStore,
    accounts: &[Account],
    password: &str,
    options: &PasswordOptions,
    settings: ValidatePasswordSettings,
    now: u64,
    op: &str,
) -> Result<(), (u16, String)> {
    for account in accounts {
        let Some(status) = store.password_status(&account.user, &account.host) else {
            return Err((
                1396,
                rusql_i18n::messages::sql_cannot_user(op, &account.display()),
            ));
        };
        if !password_satisfies_policy(password, &account.user, settings) {
            return Err((
                ER_NOT_VALID_PASSWORD,
                rusql_i18n::messages::sql_password_not_valid(),
            ));
        }
        let samples: Vec<HistorySample<'_>> = status
            .history
            .iter()
            .map(|entry| HistorySample {
                password: entry.password.as_str(),
                changed_unix: entry.changed_unix,
            })
            .collect();
        let (limit, reuse) = store.effective_reuse_limits(&status, options);
        if password_contradicts_history(
            password,
            &status.current,
            status.changed_unix,
            &samples,
            limit,
            reuse,
            now,
        ) {
            return Err((
                ER_CREDENTIALS_CONTRADICT_TO_HISTORY,
                rusql_i18n::messages::sql_password_history_contradict(&account.user, &account.host),
            ));
        }
    }
    Ok(())
}

/// Errno 1290 when `read_only` / `super_read_only` blocks a write. `Ok(true)` if rejected.
async fn reject_read_only_write<S>(
    stream: &mut S,
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
    user: &str,
) -> Result<bool, ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let role = engine.read().await.replica_role();
    let Some(option) = role.blocking_option(rusql_core::PrivilegeStore::is_superuser(user)) else {
        return Ok(false);
    };
    let err = err_packet(
        1290,
        &rusql_i18n::messages::sql_option_prevents_statement(option),
    );
    write_packets(stream, 1, &[err]).await?;
    Ok(true)
}

async fn write_exec_error<S>(stream: &mut S, error: ExecError) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let err = match error {
        ExecError::Message(m) => err_packet(1105, &m),
        ExecError::Mysql { code, message } => err_packet(code, &message),
        ExecError::SpSignal {
            code,
            sqlstate,
            message,
        } => err_packet_with_sqlstate(code, &sqlstate, &message),
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

    #[tokio::test]
    async fn flush_privileges_reloads_replaced_catalog_for_new_login() {
        use rusql_core::{Account, AUTH_PLUGIN_CACHING_SHA2};
        use rusql_protocol::client_decode::QueryResponse;
        let server = TestServer::start("flush_privileges_login").await;
        let mut admin = server.connect().await;
        assert!(matches!(
            admin
                .query("CREATE USER 'live'@'%' IDENTIFIED BY 'secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            admin.query("FLUSH PRIVILEGES").await,
            QueryResponse::Ok { .. }
        ));
        admin.quit().await;

        let mut live = server.connect_as("live", "secret").await;
        assert!(matches!(
            live.query("SELECT 1").await,
            QueryResponse::Rows { .. }
        ));
        live.quit().await;

        let mut disk = PrivilegeStore::new();
        disk.create_user(
            &Account::new("fresh", "%"),
            "secret",
            AUTH_PLUGIN_CACHING_SHA2,
            false,
        )
        .unwrap();
        disk.save(&server.data_dir).unwrap();
        assert_eq!(
            server.try_connect_as("fresh", "secret").await.unwrap_err(),
            0xFF
        );
        let mut still_live = server.connect_as("live", "secret").await;
        still_live.quit().await;

        let mut admin = server.connect().await;
        assert!(matches!(
            admin.query("FLUSH NO_WRITE_TO_BINLOG PRIVILEGES").await,
            QueryResponse::Ok { .. }
        ));
        match admin
            .query("SELECT User FROM mysql.user WHERE User = 'fresh'")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["fresh".to_string()]]);
            }
            other => panic!("mysql.user after flush: {other:?}"),
        }
        admin.quit().await;

        let mut fresh = server.connect_as("fresh", "secret").await;
        assert!(matches!(
            fresh.query("SELECT 1").await,
            QueryResponse::Rows { .. }
        ));
        fresh.quit().await;
        assert_eq!(
            server.try_connect_as("live", "secret").await.unwrap_err(),
            0xFF
        );
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn flush_privileges_rejects_other_variants_and_bad_catalog() {
        use rusql_protocol::client_decode::QueryResponse;
        let server = TestServer::start("flush_privileges_err").await;
        let mut admin = server.connect().await;
        assert!(matches!(
            admin
                .query("CREATE USER 'live'@'%' IDENTIFIED BY 'secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
        admin.quit().await;

        let mut live = server.connect_as("live", "secret").await;
        match live.query("FLUSH PRIVILEGES").await {
            QueryResponse::Err { code: 1227, .. } => {}
            other => panic!("non-root FLUSH PRIVILEGES: {other:?}"),
        }
        match live.query("FLUSH TABLES").await {
            QueryResponse::Err { code: 1227, .. } => {}
            other => panic!("non-root FLUSH TABLES: {other:?}"),
        }
        live.quit().await;

        let mut admin = server.connect().await;
        for sql in [
            "FLUSH TABLES",
            "FLUSH TABLES WITH READ LOCK",
            "FLUSH HOSTS",
            "FLUSH STATUS",
        ] {
            match admin.query(sql).await {
                QueryResponse::Err { code: 1235, .. } => {}
                other => panic!("{sql}: {other:?}"),
            }
        }
        match admin.query("FLUSH NOT_A_REAL_OPTION").await {
            QueryResponse::Err { code: 1064, .. } => {}
            other => panic!("unknown FLUSH: {other:?}"),
        }
        std::fs::write(PrivilegeStore::grants_file(&server.data_dir), b"{not-json").unwrap();
        match admin.query("FLUSH PRIVILEGES").await {
            QueryResponse::Err { code: 1105, .. } => {}
            other => panic!("corrupt catalog: {other:?}"),
        }
        admin.quit().await;
        let mut still = server.connect_as("live", "secret").await;
        assert!(matches!(
            still.query("SELECT 1").await,
            QueryResponse::Rows { .. }
        ));
        still.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn password_expire_blocks_com_query_until_alter_user() {
        use rusql_protocol::client_decode::QueryResponse;
        let server = TestServer::start("password_expire").await;
        let mut admin = server.connect().await;
        assert!(matches!(
            admin
                .query("CREATE USER 'm183'@'%' IDENTIFIED BY 'secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
        let mut user = server.connect_as("m183", "secret").await;
        assert!(
            matches!(user.query("SELECT 1").await, QueryResponse::Rows { .. }),
            "non-expired caching_sha2 login must still run COM_QUERY"
        );
        user.quit().await;
        assert!(matches!(
            admin.query("ALTER USER 'm183'@'%' PASSWORD EXPIRE").await,
            QueryResponse::Ok { .. }
        ));
        admin.quit().await;

        let mut expired = server.connect_as("m183", "secret").await;
        match expired.query("SELECT 1").await {
            QueryResponse::Err { code: 1820, .. } => {}
            other => panic!("expired COM_QUERY: {other:?}"),
        }
        match expired
            .query("ALTER USER USER() IDENTIFIED BY 'secret2'")
            .await
        {
            QueryResponse::Ok { .. } => {}
            other => panic!("self alter user: {other:?}"),
        }
        assert!(matches!(
            expired.query("SELECT 1").await,
            QueryResponse::Rows { .. }
        ));
        expired.quit().await;

        let mut again = server.connect_as("m183", "secret2").await;
        assert!(matches!(
            again.query("SELECT 1").await,
            QueryResponse::Rows { .. }
        ));
        again.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn password_expire_policy_rejects_weak_and_reused() {
        use rusql_protocol::client_decode::QueryResponse;
        let server = TestServer::start("password_policy").await;
        let mut admin = server.connect().await;
        for sql in [
            "SET GLOBAL validate_password.policy = 'LOW'",
            "SET GLOBAL validate_password.length = 8",
        ] {
            assert!(
                matches!(admin.query(sql).await, QueryResponse::Ok { .. }),
                "{sql}"
            );
        }
        match admin
            .query("CREATE USER 'weak'@'%' IDENTIFIED BY 'short'")
            .await
        {
            QueryResponse::Err { code: 1819, .. } => {}
            other => panic!("weak password: {other:?}"),
        }
        assert!(matches!(
            admin
                .query("CREATE USER 'weak'@'%' IDENTIFIED BY 'longenough'")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            admin
                .query("ALTER USER 'weak'@'%' PASSWORD HISTORY 2")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            admin
                .query("ALTER USER 'weak'@'%' IDENTIFIED BY 'brand-new'")
                .await,
            QueryResponse::Ok { .. }
        ));
        match admin
            .query("ALTER USER 'weak'@'%' IDENTIFIED BY 'longenough'")
            .await
        {
            QueryResponse::Err { code: 3638, .. } => {}
            other => panic!("reused password: {other:?}"),
        }
        assert!(matches!(
            admin
                .query("SET PASSWORD FOR 'weak'@'%' = 'another-ok'")
                .await,
            QueryResponse::Ok { .. }
        ));
        match admin
            .query("ALTER USER 'missing'@'%' PASSWORD EXPIRE")
            .await
        {
            QueryResponse::Err { code: 1396, .. } => {}
            other => panic!("missing user: {other:?}"),
        }
        match admin.query("SELECT @@validate_password.length").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows[0][0], "8"),
            other => panic!("select policy: {other:?}"),
        }
        admin.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[test]
    fn password_expire_interval_elapsed_is_must_change() {
        use rusql_core::{PasswordExpireMode, PasswordOptions, AUTH_PLUGIN_CACHING_SHA2};
        use rusql_protocol::password_lifetime_elapsed;
        let mut store = PrivilegeStore::new();
        let account = rusql_core::Account::new("a", "%");
        let options = PasswordOptions {
            expire: Some(PasswordExpireMode::Days(1)),
            ..PasswordOptions::default()
        };
        store
            .create_user_with_options(
                &account,
                "secret",
                AUTH_PLUGIN_CACHING_SHA2,
                false,
                &options,
                1_000,
            )
            .unwrap();
        let status = store.password_status("a", "%").unwrap();
        assert_eq!(status.lifetime_days, 1);
        assert!(!password_lifetime_elapsed(
            status.changed_unix,
            status.lifetime_days,
            1_000 + 86_400 - 1
        ));
        assert!(password_lifetime_elapsed(
            status.changed_unix,
            status.lifetime_days,
            1_000 + 86_400
        ));
    }

    #[tokio::test]
    async fn roles_grant_set_role_allows_select() {
        use rusql_protocol::client_decode::QueryResponse;
        let server = TestServer::start("roles_select").await;
        let mut admin = server.connect().await;
        for sql in [
            "CREATE TABLE role_t (id INT)",
            "INSERT INTO role_t VALUES (1)",
            "CREATE ROLE r",
            "GRANT SELECT ON rusql.* TO r",
            "CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'",
            "GRANT r TO 'app'@'%'",
        ] {
            assert!(
                matches!(admin.query(sql).await, QueryResponse::Ok { .. }),
                "{sql}"
            );
        }
        match admin.query("SHOW GRANTS FOR 'app'@'%'").await {
            QueryResponse::Rows { rows, .. } => {
                let text = rows.into_iter().flatten().collect::<Vec<_>>().join("\n");
                assert!(text.contains("GRANT `r`@`%`"), "{text}");
            }
            other => panic!("show grants: {other:?}"),
        }
        match admin.query("CREATE ROLE r").await {
            QueryResponse::Err { code: 1396, .. } => {}
            other => panic!("duplicate role: {other:?}"),
        }
        match admin.query("GRANT missing TO 'app'@'%'").await {
            QueryResponse::Err { code: 3523, .. } => {}
            other => panic!("unknown role: {other:?}"),
        }

        let mut app = server.connect_native_as("app", "secret").await;
        match app.query("SELECT CURRENT_ROLE()").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows[0][0], "NONE"),
            other => panic!("current role before set: {other:?}"),
        }
        match app.query("SELECT id FROM role_t").await {
            QueryResponse::Err { code: 1142, .. } => {}
            other => panic!("select before set role: {other:?}"),
        }
        assert!(matches!(
            app.query("SET ROLE r").await,
            QueryResponse::Ok { .. }
        ));
        match app.query("SELECT CURRENT_ROLE()").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows[0][0], "`r`@`%`"),
            other => panic!("current role: {other:?}"),
        }
        match app.query("SELECT id FROM role_t").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows, vec![vec!["1".to_string()]]),
            other => panic!("select via role: {other:?}"),
        }
        assert!(matches!(
            admin.query("REVOKE r FROM 'app'@'%'").await,
            QueryResponse::Ok { .. }
        ));
        match app.query("SELECT id FROM role_t").await {
            QueryResponse::Err { code: 1142, .. } => {}
            other => panic!("select after revoke: {other:?}"),
        }
        match app.query("SET ROLE r").await {
            QueryResponse::Err { code: 3530, .. } => {}
            other => panic!("set role after revoke: {other:?}"),
        }
        app.quit().await;
        assert!(matches!(
            admin.query("DROP ROLE r").await,
            QueryResponse::Ok { .. }
        ));
        admin.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{temp_data_dir, TestServer, WireClient};
    use rusql_protocol::client_decode::QueryResponse;
    use rusql_protocol::{
        encode_semi_sync_ack, is_heartbeat_dump_packet, BINLOG_DUMP_NON_BLOCK, HEARTBEAT_LOG_EVENT,
    };
    use rusql_storage::{
        apply_binlog_file, extract_query_events, PersistentEngine, StorageEngine, BINLOG_MAGIC,
        EVENT_TYPE_DELETE_ROWS_V2, EVENT_TYPE_GTID_LOG, EVENT_TYPE_HEARTBEAT, EVENT_TYPE_TABLE_MAP,
        EVENT_TYPE_UPDATE_ROWS_V2, EVENT_TYPE_WRITE_ROWS_V2,
    };

    #[tokio::test]
    async fn replica_promote_blocks_writes_then_accepts_them() {
        let server = TestServer::start("replica-promote").await;
        let mut root = server.connect().await;
        let mut app = server.connect_as("app", "").await;
        assert!(matches!(
            root.query("CREATE TABLE m177 (id INT PRIMARY KEY)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            root.query("SELECT id FROM m177").await,
            QueryResponse::Rows { .. }
        ));
        assert!(matches!(
            root.query("SET GLOBAL super_read_only = ON").await,
            QueryResponse::Ok { .. }
        ));
        match root.query("INSERT INTO m177 VALUES (1)").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1290);
                assert!(message.contains("--super-read-only"));
            }
            other => panic!("expected errno 1290 for SUPER, got {other:?}"),
        }
        assert!(matches!(
            root.query("SELECT id FROM m177").await,
            QueryResponse::Rows { .. }
        ));
        match app.query("INSERT INTO m177 VALUES (1)").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1290),
            other => panic!("expected errno 1290 for non-SUPER, got {other:?}"),
        }
        match root.query("SET read_only = OFF").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1229),
            other => panic!("expected errno 1229, got {other:?}"),
        }
        assert!(matches!(
            root.query("SET GLOBAL read_only = OFF").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            root.query("INSERT INTO m177 VALUES (1)").await,
            QueryResponse::Ok { .. }
        ));
        let reopened = server.reopen_engine();
        assert!(reopened.replica_role().writes_allowed(false));

        assert!(matches!(
            root.query("SET GLOBAL read_only = ON").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            root.query("INSERT INTO m177 VALUES (2)").await,
            QueryResponse::Ok { .. }
        ));
        match app.query("INSERT INTO m177 VALUES (3)").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1290);
                assert!(message.contains("--read-only"));
            }
            other => panic!("expected errno 1290 under read_only, got {other:?}"),
        }
        assert!(matches!(
            root.query("SET GLOBAL read_only = OFF").await,
            QueryResponse::Ok { .. }
        ));
    }

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

    /// M189: performance_schema stub tables are selectable. Other names stay errno 1146.
    #[tokio::test]
    async fn performance_schema_stub_on_wire() {
        let server = TestServer::start("performance_schema").await;
        let mut client = server.connect().await;

        match client
            .query("SELECT VARIABLE_NAME, VARIABLE_VALUE FROM performance_schema.session_variables WHERE VARIABLE_NAME = 'autocommit'")
            .await
        {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec!["VARIABLE_NAME".to_string(), "VARIABLE_VALUE".to_string()]
                );
                assert_eq!(rows, vec![vec!["autocommit".to_string(), "1".to_string()]]);
            }
            other => panic!("session_variables must not be errno 1146, got {other:?}"),
        }
        match client
            .query("SELECT * FROM performance_schema.accounts")
            .await
        {
            QueryResponse::Rows { columns, rows } => {
                assert!(columns.iter().any(|c| c == "CURRENT_CONNECTIONS"));
                assert!(columns.iter().any(|c| c == "TOTAL_CONNECTIONS"));
                assert!(rows.is_empty());
            }
            other => panic!("accounts must be empty rows, got {other:?}"),
        }
        match client
            .query("SELECT COUNT_STAR FROM performance_schema.events_statements_summary_by_digest")
            .await
        {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("digest stub must not be errno 1146, got {other:?}"),
        }
        match client
            .query("SELECT THREAD_ID FROM performance_schema.threads")
            .await
        {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("threads must not be errno 1146, got {other:?}"),
        }
        match client
            .query("SELECT THREAD_ID FROM performance_schema.events_waits_current")
            .await
        {
            QueryResponse::Err { code: 1146, .. } => {}
            other => panic!("events_waits_current must be errno 1146, got {other:?}"),
        }
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M110: TRUNCATE TABLE empties the heap, resets AUTO_INCREMENT, errno 1146 for unknown tables.
    #[tokio::test]
    async fn truncate_table_wire() {
        let server = TestServer::start("truncate_table").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client.query("CREATE TABLE tr_t (id INT PRIMARY KEY)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO tr_t VALUES (1),(2)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("TRUNCATE TABLE tr_t").await,
            QueryResponse::Ok { affected_rows: 0 }
        ));
        match client.query("SELECT id FROM tr_t").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty SELECT after truncate, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE TABLE tr_ai (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(16))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO tr_ai (name) VALUES ('a')").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("TRUNCATE tr_ai").await,
            QueryResponse::Ok { affected_rows: 0 }
        ));
        assert!(matches!(
            client.query("INSERT INTO tr_ai (name) VALUES ('b')").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT id FROM tr_ai").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected id 1 after truncate, got {other:?}"),
        }
        match client.query("SELECT LAST_INSERT_ID()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected LAST_INSERT_ID 1, got {other:?}"),
        }

        assert!(matches!(
            client.query("TRUNCATE TABLE no_such_truncate").await,
            QueryResponse::Err { code: 1146, .. }
        ));
        assert!(matches!(
            client
                .query("TRUNCATE TABLE information_schema.tables")
                .await,
            QueryResponse::Err { code: 1146, .. }
        ));

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
    async fn read_committed_two_connections() {
        let server = TestServer::start("read_committed").await;
        let mut setup = server.connect().await;
        assert!(matches!(
            setup
                .query("CREATE TABLE rc_iso (id INT, v VARCHAR(8))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            setup.query("INSERT INTO rc_iso VALUES (1, 'a')").await,
            QueryResponse::Ok { .. }
        ));
        setup.quit().await;

        let mut reader = server.connect().await;
        assert!(matches!(
            reader
                .query("SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            reader.query("BEGIN").await,
            QueryResponse::Ok { .. }
        ));
        match reader.query("SELECT v FROM rc_iso WHERE id = 1").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["a".to_string()]]);
            }
            other => panic!("expected first RC read, got {other:?}"),
        }

        let mut writer = server.connect().await;
        assert!(matches!(
            writer.query("INSERT INTO rc_iso VALUES (2, 'b')").await,
            QueryResponse::Ok { .. }
        ));
        writer.quit().await;

        match reader.query("SELECT id FROM rc_iso ORDER BY id").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["1".to_string()], vec!["2".to_string()]],
                    "READ COMMITTED txn must see committed inserts from other connections"
                );
            }
            other => panic!("expected second RC read with new row, got {other:?}"),
        }
        assert!(matches!(
            reader.query("COMMIT").await,
            QueryResponse::Ok { .. }
        ));
        reader.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M159: plain `SELECT` under SERIALIZABLE takes row locks like `FOR UPDATE`.
    #[tokio::test]
    async fn serializable_plain_select_holds_row_lock() {
        let server = TestServer::start("serializable_lock").await;
        let mut setup = server.connect().await;
        assert!(matches!(
            setup
                .query("CREATE TABLE ser_lock (id INT PRIMARY KEY, v VARCHAR(8))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            setup.query("INSERT INTO ser_lock VALUES (1, 'a')").await,
            QueryResponse::Ok { .. }
        ));
        setup.quit().await;

        let mut a = server.connect().await;
        assert!(matches!(
            a.query("SET SESSION TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(a.query("BEGIN").await, QueryResponse::Ok { .. }));
        match a.query("SELECT id FROM ser_lock WHERE id = 1").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected SERIALIZABLE read, got {other:?}"),
        }
        let lock_key = rusql_storage::RowKey::new("ser_lock", vec!["1".into()]);
        assert!(
            server
                .row_locks
                .as_ref()
                .is_held_by(a.connection_id, &lock_key),
            "SERIALIZABLE plain SELECT should hold exclusive row lock until COMMIT"
        );

        let addr = server.addr;
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut b = WireClient::connect_addr(addr).await;
            assert!(matches!(b.query("BEGIN").await, QueryResponse::Ok { .. }));
            let resp = b
                .query("UPDATE ser_lock SET v = 'blocked' WHERE id = 1")
                .await;
            let _ = tx.send(resp);
            b.quit().await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(
            rx.try_recv().is_err(),
            "UPDATE on SERIALIZABLE-locked row should block until COMMIT"
        );
        assert!(matches!(a.query("COMMIT").await, QueryResponse::Ok { .. }));

        let b_resp = tokio::time::timeout(std::time::Duration::from_secs(3), rx)
            .await
            .expect("waiter timed out")
            .expect("waiter dropped");
        assert!(
            matches!(b_resp, QueryResponse::Ok { .. }),
            "UPDATE proceeds after SERIALIZABLE holder COMMIT, got {b_resp:?}"
        );

        a.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M159: SERIALIZABLE + RR snapshot — committed insert after first scan is not a phantom on rescan.
    #[tokio::test]
    async fn serializable_no_phantom_on_rescan() {
        let server = TestServer::start("serializable_phantom").await;
        let mut setup = server.connect().await;
        assert!(matches!(
            setup
                .query("CREATE TABLE ser_ph (id INT PRIMARY KEY, v VARCHAR(8))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            setup.query("INSERT INTO ser_ph VALUES (1, 'a')").await,
            QueryResponse::Ok { .. }
        ));
        setup.quit().await;

        let mut reader = server.connect().await;
        assert!(matches!(
            reader
                .query("SET SESSION TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            reader.query("BEGIN").await,
            QueryResponse::Ok { .. }
        ));
        // Filter on non-indexed column forces heap scan so M32 snapshot pins (index-only paths may bypass).
        match reader
            .query("SELECT id FROM ser_ph WHERE v IS NOT NULL ORDER BY id")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected first SERIALIZABLE scan, got {other:?}"),
        }

        let mut writer = server.connect().await;
        assert!(matches!(
            writer.query("INSERT INTO ser_ph VALUES (2, 'b')").await,
            QueryResponse::Ok { .. }
        ));
        writer.quit().await;

        match reader
            .query("SELECT id FROM ser_ph WHERE v IS NOT NULL ORDER BY id")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["1".to_string()]],
                    "SERIALIZABLE txn must not see phantom row committed after first scan"
                );
            }
            other => panic!("expected second SERIALIZABLE scan without phantom, got {other:?}"),
        }
        assert!(matches!(
            reader.query("COMMIT").await,
            QueryResponse::Ok { .. }
        ));
        reader.quit().await;
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

    /// M126: SAVEPOINT / ROLLBACK TO / RELEASE SAVEPOINT inside a transaction.
    #[tokio::test]
    async fn savepoint_rollback_to_and_release() {
        let server = TestServer::start("savepoint").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE TABLE gap_sp (id INT PRIMARY KEY)")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("BEGIN").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO gap_sp VALUES (1)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("SAVEPOINT gap_sp1").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO gap_sp VALUES (2)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("ROLLBACK TO SAVEPOINT gap_sp1").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT id FROM gap_sp ORDER BY id").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["1".to_string()]],
                    "ROLLBACK TO must keep the pre-savepoint row and leave the txn open"
                );
            }
            other => panic!("expected remaining savepoint row, got {other:?}"),
        }
        assert!(matches!(
            client.query("RELEASE SAVEPOINT gap_sp1").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("ROLLBACK TO SAVEPOINT gap_sp1").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1305);
                assert!(
                    message.contains("gap_sp1"),
                    "errno 1305 should name the savepoint, got {message}"
                );
            }
            other => panic!("expected errno 1305 after RELEASE, got {other:?}"),
        }
        assert!(matches!(
            client.query("ROLLBACK").await,
            QueryResponse::Ok { .. }
        ));

        assert!(matches!(
            client.query("SAVEPOINT gap_sp1").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO gap_sp VALUES (3)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("ROLLBACK TO SAVEPOINT gap_sp1").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT id FROM gap_sp").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows.is_empty(),
                    "SAVEPOINT outside BEGIN must start a transaction, got {rows:?}"
                );
            }
            other => panic!("expected empty after implicit-start ROLLBACK TO, got {other:?}"),
        }
        client.quit().await;

        let mut other = server.connect().await;
        match other.query("SELECT id FROM gap_sp").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows.is_empty(),
                    "uncommitted savepoint txn must not persist, got {rows:?}"
                );
            }
            other => panic!("expected empty table after disconnect, got {other:?}"),
        }
        other.quit().await;
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

    /// M162: XA two-phase commit with WAL-persisted PREPARE and restart recovery.
    #[tokio::test]
    async fn xa_prepare_recover_after_restart() {
        let server = TestServer::start("xa_m162").await;
        let mut client = server.connect().await;
        let xid = "m162_restart_xid";

        assert!(matches!(
            client
                .query("CREATE TABLE xa_rec (id INT PRIMARY KEY)")
                .await,
            QueryResponse::Ok { .. }
        ));
        let xa_start = format!("XA START '{xid}'");
        assert!(matches!(
            client.query(xa_start.as_str()).await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO xa_rec VALUES (99)").await,
            QueryResponse::Ok { .. }
        ));
        let xa_end = format!("XA END '{xid}'");
        assert!(matches!(
            client.query(xa_end.as_str()).await,
            QueryResponse::Ok { .. }
        ));
        let xa_prepare = format!("XA PREPARE '{xid}'");
        assert!(matches!(
            client.query(xa_prepare.as_str()).await,
            QueryResponse::Ok { .. }
        ));
        match client.query("XA RECOVER").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows.iter()
                        .any(|r| r.get(1).map(|s| s.as_str()) == Some(xid)),
                    "XA RECOVER must list prepared xid, got {rows:?}"
                );
            }
            other => panic!("expected XA RECOVER rows, got {other:?}"),
        }
        match client.query("SELECT id FROM xa_rec").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows.is_empty(), "prepared XA must not commit rows yet");
            }
            other => panic!("expected empty scan before XA COMMIT, got {other:?}"),
        }
        client.quit().await;

        let eng = server.reopen_engine();
        assert!(eng.xa_is_prepared(xid));
        assert!(eng.scan("xa_rec").unwrap().is_empty());

        let mut client2 = server.connect().await;
        let xa_commit = format!("XA COMMIT '{xid}'");
        assert!(matches!(
            client2.query(xa_commit.as_str()).await,
            QueryResponse::Ok { .. }
        ));
        match client2.query("SELECT id FROM xa_rec").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["99".to_string()]]);
            }
            other => panic!("expected row after XA COMMIT, got {other:?}"),
        }
        match client2.query("XA PREPARE 'm162_restart_xid'").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1399),
            other => panic!("expected errno 1399 for illegal XA PREPARE, got {other:?}"),
        }

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

    /// M117: LAST_INSERT_ID(expr) sets the session value used by later LAST_INSERT_ID().
    #[tokio::test]
    async fn last_insert_id_expr_sets_session() {
        let server = TestServer::start("last_insert_id_expr").await;
        let mut a = server.connect().await;
        let mut b = server.connect().await;

        match a.query("SELECT LAST_INSERT_ID(5)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["5".to_string()]]);
            }
            other => panic!("expected LAST_INSERT_ID(5), got {other:?}"),
        }
        match a.query("SELECT LAST_INSERT_ID()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["5".to_string()]]);
            }
            other => panic!("expected LAST_INSERT_ID() 5, got {other:?}"),
        }
        match a.query("SELECT LAST_INSERT_ID(5.9)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["6".to_string()]]);
            }
            other => panic!("expected LAST_INSERT_ID(5.9), got {other:?}"),
        }

        assert!(matches!(
            a.query("CREATE TABLE li_ok (id INT)").await,
            QueryResponse::Ok { .. }
        ));
        assert_eq!(
            a.last_ok_insert_id, 6,
            "SELECT setter updates session.last_insert_id used by later OK packets"
        );

        match b.query("SELECT LAST_INSERT_ID()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["0".to_string()]],
                    "second connection must not see first connection LAST_INSERT_ID(expr)"
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

    fn get_lock_cell(resp: QueryResponse) -> String {
        match resp {
            QueryResponse::Rows { rows, .. } => rows[0][0].clone(),
            other => panic!("expected GET_LOCK/RELEASE_LOCK rows, got {other:?}"),
        }
    }

    /// M118: GET_LOCK / RELEASE_LOCK are process-wide advisory locks, session-scoped holders.
    #[tokio::test]
    async fn get_lock_two_connections_release_reset_and_disconnect() {
        let server = TestServer::start("get_lock").await;
        let mut a = server.connect().await;
        let mut b = server.connect().await;

        assert_eq!(
            get_lock_cell(a.query("SELECT GET_LOCK('gap_lock', 0)").await),
            "1"
        );
        assert_eq!(
            get_lock_cell(a.query("SELECT GET_LOCK('gap_lock', 0)").await),
            "1",
            "re-GET_LOCK on the holder still returns 1"
        );
        assert_eq!(
            get_lock_cell(b.query("SELECT GET_LOCK('gap_lock', 0)").await),
            "0"
        );
        assert_eq!(
            get_lock_cell(b.query("SELECT IS_FREE_LOCK('gap_lock')").await),
            "0"
        );
        assert_eq!(
            get_lock_cell(b.query("SELECT IS_USED_LOCK('gap_lock')").await),
            a.connection_id.to_string()
        );
        assert_eq!(
            get_lock_cell(b.query("SELECT RELEASE_LOCK('gap_lock')").await),
            "0",
            "non-holder RELEASE_LOCK is 0 while someone else holds it"
        );
        assert_eq!(
            get_lock_cell(a.query("SELECT RELEASE_LOCK('gap_lock')").await),
            "1"
        );
        assert_eq!(
            get_lock_cell(b.query("SELECT RELEASE_LOCK('gap_lock')").await),
            "",
            "RELEASE_LOCK of a free name is NULL"
        );
        assert_eq!(
            get_lock_cell(b.query("SELECT GET_LOCK('gap_lock', 0)").await),
            "1"
        );
        match a.query("SELECT GET_LOCK(NULL, 0)").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 3057);
                assert!(
                    message.to_ascii_lowercase().contains("lock") || message.contains("锁"),
                    "expected i18n invalid lock name, got {message}"
                );
            }
            other => panic!("expected errno 3057 for GET_LOCK(NULL), got {other:?}"),
        }
        match a.query("SELECT RELEASE_LOCK(NULL)").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 3057);
                assert!(
                    message.to_ascii_lowercase().contains("lock") || message.contains("锁"),
                    "expected i18n invalid lock name, got {message}"
                );
            }
            other => panic!("expected errno 3057 for RELEASE_LOCK(NULL), got {other:?}"),
        }

        assert!(matches!(
            b.reset_connection().await,
            QueryResponse::Ok { .. }
        ));
        assert_eq!(
            get_lock_cell(a.query("SELECT GET_LOCK('gap_lock', 0)").await),
            "1",
            "COM_RESET_CONNECTION must release the reset session's locks"
        );

        a.change_user("root", "", "rusql").await;
        assert_eq!(
            get_lock_cell(b.query("SELECT GET_LOCK('gap_lock', 0)").await),
            "1",
            "COM_CHANGE_USER must release the previous session's locks"
        );

        drop(b);
        let mut acquired = false;
        for _ in 0..100 {
            let cell = get_lock_cell(a.query("SELECT GET_LOCK('gap_lock', 0)").await);
            if cell == "1" {
                acquired = true;
                break;
            }
            assert_eq!(cell, "0");
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert!(
            acquired,
            "disconnect must release locks held by the closed connection"
        );

        a.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M164: `GET_LOCK(name, timeout>0)` blocks up to timeout seconds on a second connection.
    #[tokio::test]
    async fn get_lock_timeout_wait_release_and_expiry() {
        let server = TestServer::start("get_lock_timeout").await;
        let mut holder = server.connect().await;
        let mut waiter = server.connect().await;

        assert_eq!(
            get_lock_cell(holder.query("SELECT GET_LOCK('m164_wait', 0)").await),
            "1"
        );
        let start = std::time::Instant::now();
        assert_eq!(
            get_lock_cell(waiter.query("SELECT GET_LOCK('m164_wait', 1)").await),
            "0",
            "timeout>0 should wait about 1s then return 0 while lock is held"
        );
        assert!(
            start.elapsed() >= std::time::Duration::from_millis(900),
            "expected ~1s wait, got {:?}",
            start.elapsed()
        );
        assert_eq!(
            get_lock_cell(holder.query("SELECT RELEASE_LOCK('m164_wait')").await),
            "1"
        );
        assert_eq!(
            get_lock_cell(waiter.query("SELECT GET_LOCK('m164_wait', 0)").await),
            "1",
            "RELEASE_LOCK from holder must unblock a later non-blocking GET_LOCK"
        );

        holder.quit().await;
        waiter.quit().await;
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

    /// M86/M190: SHOW STATUS catalog. Questions, Uptime, and Threads_connected are live.
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
                let _: u64 = rows
                    .iter()
                    .find(|r| r[0] == "Uptime")
                    .expect("Uptime")
                    .get(1)
                    .expect("value")
                    .parse()
                    .expect("Uptime numeric");
                assert!(rows
                    .iter()
                    .any(|r| r[0] == "Threads_connected" && r[1] == "1"));
                assert!(rows
                    .iter()
                    .any(|r| r[0] == "Threads_running" && r[1] == "1"));
                let questions: u64 = rows
                    .iter()
                    .find(|r| r[0] == "Questions")
                    .expect("Questions")
                    .get(1)
                    .expect("value")
                    .parse()
                    .expect("Questions numeric");
                assert!(questions >= 1, "COM_QUERY must increment Questions");
                assert!(rows.iter().any(|r| r[0] == "Slow_queries" && r[1] == "0"));
                rows
            }
            other => panic!("expected SHOW STATUS rows, got {other:?}"),
        };

        let names: Vec<_> = full.iter().map(|r| r[0].as_str()).collect();
        match client.query("SHOW SESSION STATUS").await {
            QueryResponse::Rows { rows, .. } => {
                let session_names: Vec<_> = rows.iter().map(|r| r[0].as_str()).collect();
                assert_eq!(session_names, names);
                assert!(rows.iter().any(|r| r[0] == "Slow_queries" && r[1] == "0"));
            }
            other => panic!("expected SHOW SESSION STATUS rows, got {other:?}"),
        }
        match client.query("SHOW GLOBAL STATUS").await {
            QueryResponse::Rows { rows, .. } => {
                let global_names: Vec<_> = rows.iter().map(|r| r[0].as_str()).collect();
                assert_eq!(global_names, names);
                assert!(rows.iter().any(|r| r[0] == "Bytes_sent" && r[1] == "0"));
            }
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

    /// M190: live Questions (per COM_QUERY) and monotonic Uptime.
    #[tokio::test]
    async fn show_status_live_questions_and_uptime() {
        let server = TestServer::start("show_status_live").await;
        let mut client = server.connect().await;

        let q1 = match client.query("SHOW STATUS LIKE 'Questions'").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec!["Variable_name".to_string(), "Value".to_string()]
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "Questions");
                rows[0][1].parse::<u64>().expect("Questions numeric")
            }
            other => panic!("expected Questions row, got {other:?}"),
        };
        let q2 = match client.query("SHOW STATUS LIKE 'Questions'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                rows[0][1].parse::<u64>().expect("Questions numeric")
            }
            other => panic!("expected Questions row, got {other:?}"),
        };
        assert!(
            q2 > q1,
            "second Questions ({q2}) must be >= first ({q1}) + 1"
        );

        let session_q = match client.query("SHOW SESSION STATUS LIKE 'Questions'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "Questions");
                rows[0][1].parse::<u64>().expect("Questions numeric")
            }
            other => panic!("expected session Questions row, got {other:?}"),
        };
        assert!(session_q > q2);
        let global_q = match client.query("SHOW GLOBAL STATUS LIKE 'Questions'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "Questions");
                rows[0][1].parse::<u64>().expect("Questions numeric")
            }
            other => panic!("expected global Questions row, got {other:?}"),
        };
        assert!(global_q > session_q);

        let u1 = match client.query("SHOW STATUS LIKE 'Uptime'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "Uptime");
                rows[0][1].parse::<u64>().expect("Uptime numeric")
            }
            other => panic!("expected Uptime row, got {other:?}"),
        };
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let u2 = match client.query("SHOW GLOBAL STATUS LIKE 'Uptime'").await {
            QueryResponse::Rows { rows, .. } => rows[0][1].parse::<u64>().expect("Uptime numeric"),
            other => panic!("expected Uptime row, got {other:?}"),
        };
        assert!(u2 > u1, "Uptime after sleep ({u2}) must be > first ({u1})");

        match client.query("SHOW STATUS LIKE 'Slow_queries'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["Slow_queries".to_string(), "0".to_string()]]
                );
            }
            other => panic!("expected Slow_queries stub, got {other:?}"),
        }
        match client.query("SHOW STATUS LIKE 'Com%'").await {
            QueryResponse::Rows { rows, .. } => assert!(
                rows.is_empty(),
                "Com_* names stay out of this catalog, got {rows:?}"
            ),
            other => panic!("expected empty Com% rows, got {other:?}"),
        }

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
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "Uptime");
                let _: u64 = rows[0][1].parse().expect("Uptime numeric");
            }
            other => panic!("SHOW STATUS must still return Uptime, got {other:?}"),
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
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "Uptime");
                let _: u64 = rows[0][1].parse().expect("Uptime numeric");
            }
            other => panic!("SHOW STATUS must still return Uptime, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M131: SHOW ENGINE INNODB STATUS documented stub (not live mutex/lock stats).
    #[tokio::test]
    async fn show_engine_innodb_status_stub() {
        let server = TestServer::start("show_engine_innodb").await;
        let mut client = server.connect().await;

        match client.query("SHOW ENGINE INNODB STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec!["Type".to_string(), "Name".to_string(), "Status".to_string()]
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "InnoDB");
                assert_eq!(rows[0][1], "");
                assert!(!rows[0][2].is_empty());
            }
            other => panic!("expected SHOW ENGINE INNODB STATUS rows, got {other:?}"),
        }

        match client.query("SHOW ENGINE MUSQL STATUS").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1286);
                assert!(message.contains("MUSQL"));
            }
            other => panic!("expected errno 1286 for unknown engine, got {other:?}"),
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

    /// M122: SHOW BINARY LOGS lists known binlog files (at least the current file).
    #[tokio::test]
    async fn show_binary_logs_lists_current_file() {
        let server = TestServer::start("show_binary_logs").await;
        let mut client = server.connect().await;

        let rows = match client.query("SHOW BINARY LOGS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec!["Log_name".to_string(), "File_size".to_string()]
                );
                assert!(
                    !rows.is_empty(),
                    "binlog enabled servers list at least one file"
                );
                assert_eq!(rows[0][0], "binlog.000001");
                assert!(
                    rows[0][1].parse::<u64>().unwrap() > 0,
                    "File_size should be the on-disk length"
                );
                rows
            }
            other => panic!("expected SHOW BINARY LOGS rows, got {other:?}"),
        };

        match client.query("SHOW MASTER LOGS").await {
            QueryResponse::Rows {
                columns,
                rows: synonym,
            } => {
                assert_eq!(
                    columns,
                    vec!["Log_name".to_string(), "File_size".to_string()]
                );
                assert_eq!(synonym, rows);
            }
            other => panic!("expected SHOW MASTER LOGS rows, got {other:?}"),
        }

        match client.query("SHOW BINLOG EVENTS LIMIT 1").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec![
                        "Log_name".to_string(),
                        "Pos".to_string(),
                        "Event_type".to_string(),
                        "Server_id".to_string(),
                        "End_log_pos".to_string(),
                        "Info".to_string(),
                    ]
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "binlog.000001");
                assert_eq!(rows[0][2], "Format_desc");
            }
            other => panic!("expected SHOW BINLOG EVENTS LIMIT 1 rows, got {other:?}"),
        }

        match client.query("SHOW MASTER STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec![
                        "File".to_string(),
                        "Position".to_string(),
                        "Binlog_Do_DB".to_string(),
                        "Binlog_Ignore_DB".to_string(),
                        "Executed_Gtid_Set".to_string(),
                    ]
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "binlog.000001");
                assert!(rows[0][1].parse::<u32>().unwrap() > 0);
            }
            other => panic!("SHOW MASTER STATUS must stay unchanged, got {other:?}"),
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

    /// M123: SHOW BINLOG EVENTS lists real events from the current binlog file.
    #[tokio::test]
    async fn show_binlog_events_lists_real_events() {
        let server = TestServer::start("show_binlog_events").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE be_t (id INT)",
            "BEGIN",
            "INSERT INTO be_t VALUES (1)",
            "COMMIT",
        ] {
            let resp = client.query(sql).await;
            assert!(
                matches!(resp, QueryResponse::Ok { .. }),
                "failed: {sql} -> {resp:?}"
            );
        }

        match client.query("SHOW BINLOG EVENTS LIMIT 1").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec![
                        "Log_name".to_string(),
                        "Pos".to_string(),
                        "Event_type".to_string(),
                        "Server_id".to_string(),
                        "End_log_pos".to_string(),
                        "Info".to_string(),
                    ]
                );
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][0], "binlog.000001");
                assert_eq!(rows[0][2], "Format_desc");
                assert!(rows[0][4].parse::<u32>().unwrap() > 4);
            }
            other => panic!("expected SHOW BINLOG EVENTS LIMIT 1 rows, got {other:?}"),
        }

        match client.query("SHOW BINLOG EVENTS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[2], "Event_type");
                assert!(rows.iter().any(|r| r[2] == "Format_desc"));
                assert!(
                    rows.iter().any(|r| r[2] == "Table_map"),
                    "expected Table_map after COMMIT, got {rows:?}"
                );
                assert!(
                    rows.iter().any(|r| r[2] == "Write_rows"),
                    "expected Write_rows after COMMIT, got {rows:?}"
                );
            }
            other => panic!("expected SHOW BINLOG EVENTS rows, got {other:?}"),
        }

        match client.query("SHOW BINARY LOGS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec!["Log_name".to_string(), "File_size".to_string()]
                );
                assert_eq!(rows[0][0], "binlog.000001");
            }
            other => panic!("SHOW BINARY LOGS must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M175: SHOW MASTER STATUS / SHOW BINARY LOG STATUS return live File + Position.
    #[tokio::test]
    async fn show_master_status_position_advances_after_insert() {
        let server = TestServer::start("show_master_status").await;
        let mut client = server.connect().await;

        let pos_before = match client.query("SHOW MASTER STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "File");
                assert_eq!(rows.len(), 1);
                rows[0][1].parse::<u32>().expect("Position")
            }
            other => panic!("expected SHOW MASTER STATUS rows, got {other:?}"),
        };

        for sql in [
            "CREATE TABLE ms_t (id INT PRIMARY KEY)",
            "BEGIN",
            "INSERT INTO ms_t VALUES (1)",
            "COMMIT",
        ] {
            let resp = client.query(sql).await;
            assert!(
                matches!(resp, QueryResponse::Ok { .. }),
                "failed: {sql} -> {resp:?}"
            );
        }

        match client.query("SHOW MASTER STATUS").await {
            QueryResponse::Rows { rows, .. } => {
                let pos_after: u32 = rows[0][1].parse().expect("Position");
                assert!(pos_after > pos_before, "{pos_after} vs {pos_before}");
                assert_eq!(rows[0][0], "binlog.000001");
                assert!(
                    !rows[0][4].is_empty(),
                    "Executed_Gtid_Set after COMMIT (M176)"
                );
            }
            other => panic!("expected SHOW MASTER STATUS after INSERT, got {other:?}"),
        }

        match client.query("SHOW BINARY LOG STATUS").await {
            QueryResponse::Rows { rows, .. } => {
                let pos_syn: u32 = rows[0][1].parse().expect("Position");
                assert!(pos_syn > pos_before);
            }
            other => panic!("expected SHOW BINARY LOG STATUS rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M179: CHANGE MASTER persists; START SLAVE dumps and applies; STOP clears flags.
    #[tokio::test]
    async fn change_master_start_stop_slave_status() {
        let primary = TestServer::start("m179_primary").await;
        let replica = TestServer::start("m179_replica").await;
        let mut primary_client = primary.connect().await;
        let mut client = replica.connect().await;

        match client.query("START SLAVE").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1200),
            other => panic!("expected errno 1200 before CHANGE MASTER, got {other:?}"),
        }

        for sql in [
            "CREATE TABLE m179_t (id INT PRIMARY KEY)",
            "BEGIN",
            "INSERT INTO m179_t VALUES (1)",
            "COMMIT",
        ] {
            assert!(
                matches!(primary_client.query(sql).await, QueryResponse::Ok { .. }),
                "primary setup failed: {sql}"
            );
        }
        assert!(
            matches!(
                client
                    .query("CREATE TABLE m179_t (id INT PRIMARY KEY)")
                    .await,
                QueryResponse::Ok { .. }
            ),
            "replica table must exist before apply"
        );

        let port = primary.addr.port();
        match client
            .query(&format!(
                "CHANGE MASTER TO MASTER_HOST='127.0.0.1', MASTER_PORT={port}, MASTER_USER='root'"
            ))
            .await
        {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected CHANGE MASTER OK, got {other:?}"),
        }

        match client.query("SHOW SLAVE STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert!(columns.contains(&"Slave_IO_Running".into()));
                assert!(columns.contains(&"Slave_SQL_Running".into()));
                assert!(columns.contains(&"Last_IO_Error".into()));
                assert!(columns.contains(&"Last_SQL_Error".into()));
                assert_eq!(rows[0][0], "127.0.0.1");
                assert_eq!(rows[0][3], "No");
                assert_eq!(rows[0][4], "No");
            }
            other => panic!("expected SHOW SLAVE STATUS rows, got {other:?}"),
        }

        match client.query("START SLAVE").await {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected START SLAVE OK, got {other:?}"),
        }
        match client.query("SHOW REPLICA STATUS").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows[0][3], "Yes");
                assert_eq!(rows[0][4], "Yes");
            }
            other => panic!("expected SHOW REPLICA STATUS rows, got {other:?}"),
        }

        match client
            .query("CHANGE MASTER TO MASTER_HOST='10.0.0.1', MASTER_USER='root'")
            .await
        {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1198),
            other => panic!("expected errno 1198 while replica is running, got {other:?}"),
        }

        let mut applied = false;
        let mut last_status = String::new();
        for _ in 0..50 {
            match client.query("SELECT id FROM m179_t").await {
                QueryResponse::Rows { rows, .. }
                    if rows
                        .iter()
                        .any(|r| r.first().map(String::as_str) == Some("1")) =>
                {
                    applied = true;
                    break;
                }
                other => {
                    last_status = format!("{other:?}");
                }
            }
            if let QueryResponse::Rows { rows, .. } = client.query("SHOW SLAVE STATUS").await {
                if !rows.is_empty() {
                    last_status = format!(
                        "io={} sql={} io_err={} sql_err={}",
                        rows[0][3], rows[0][4], rows[0][5], rows[0][6]
                    );
                    if !rows[0][5].is_empty() || !rows[0][6].is_empty() {
                        break;
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(applied, "replica did not apply primary row: {last_status}");

        match client.query("SHOW SLAVE STATUS").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows[0][3], "Yes", "IO should stay running after apply");
                assert_eq!(rows[0][4], "Yes", "SQL should stay running after apply");
                assert!(rows[0][5].is_empty(), "Last_IO_Error: {}", rows[0][5]);
                assert!(rows[0][6].is_empty(), "Last_SQL_Error: {}", rows[0][6]);
            }
            other => panic!("expected status after apply, got {other:?}"),
        }

        match client.query("STOP SLAVE").await {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected STOP SLAVE OK, got {other:?}"),
        }
        match client.query("SHOW SLAVE STATUS").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows[0][3], "No");
                assert_eq!(rows[0][4], "No");
            }
            other => panic!("expected stopped status, got {other:?}"),
        }

        match client.query("CHANGE MASTER TO MASTER_PORT=0").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1231),
            other => panic!("expected errno 1231 for port 0, got {other:?}"),
        }
        match client
            .query("CHANGE MASTER TO MASTER_HOST='127.0.0.1', MASTER_PORT=1, MASTER_USER='root'")
            .await
        {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected CHANGE MASTER OK, got {other:?}"),
        }
        match client.query("START REPLICA").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 2003);
                assert!(!message.is_empty(), "bad host must yield documented error");
            }
            other => panic!("expected START REPLICA errno 2003 for bad port, got {other:?}"),
        }

        client.quit().await;
        primary_client.quit().await;
        let _ = std::fs::remove_dir_all(&replica.data_dir);
        let _ = std::fs::remove_dir_all(&primary.data_dir);
    }

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

    /// M114: CREATE DATABASE CHARACTER SET / COLLATE persist into SHOW CREATE / SCHEMATA.
    #[tokio::test]
    async fn create_database_charset_persists() {
        let server = TestServer::start("create_database_charset").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE DATABASE gap_cs CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE DATABASE gap_cs").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Database");
                assert_eq!(rows[0][0], "gap_cs");
                assert!(rows[0][1].contains("utf8mb4"));
                assert!(rows[0][1].contains("utf8mb4_unicode_ci"));
            }
            other => panic!("expected SHOW CREATE DATABASE rows, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE DATABASE gap_cs2 CHARSET utf8mb4 COLLATE utf8mb4_0900_ai_ci")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE DATABASE gap_cs2").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows[0][1].contains("utf8mb4_0900_ai_ci"));
            }
            other => panic!("expected live collation, got {other:?}"),
        }

        assert!(matches!(
            client.query("CREATE DATABASE plain_db").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE DATABASE plain_db").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows[0][1].contains("utf8mb4_unicode_ci"));
            }
            other => panic!("expected default collation, got {other:?}"),
        }

        match client
            .query("SELECT SCHEMA_NAME, DEFAULT_CHARACTER_SET_NAME, DEFAULT_COLLATION_NAME FROM information_schema.SCHEMATA")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                let gap = rows.iter().find(|r| r[0] == "gap_cs").unwrap();
                assert_eq!(gap[1], "utf8mb4");
                assert_eq!(gap[2], "utf8mb4_unicode_ci");
                let ai = rows.iter().find(|r| r[0] == "gap_cs2").unwrap();
                assert_eq!(ai[2], "utf8mb4_0900_ai_ci");
            }
            other => panic!("expected SCHEMATA rows, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE DATABASE bad_cs CHARACTER SET latin1")
                .await,
            QueryResponse::Err { code: 1115, .. }
        ));
        assert!(matches!(
            client
                .query("CREATE DATABASE bad_col COLLATE no_such_collation")
                .await,
            QueryResponse::Err { code: 1273, .. }
        ));

        match client.query("SHOW CREATE DATABASE rusql").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows[0][1].contains("utf8mb4_unicode_ci"));
            }
            other => panic!("SHOW CREATE DATABASE rusql must stay utf8mb4, got {other:?}"),
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

    /// M124: CREATE OR REPLACE VIEW creates/replaces the stored SELECT; table names stay errno 1347.
    #[tokio::test]
    async fn or_replace_view_wire() {
        let server = TestServer::start("or_replace_view").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE TABLE gap_v (id INT PRIMARY KEY, label VARCHAR(16))")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO gap_v VALUES (1, 'a')").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE OR REPLACE VIEW gap_vw AS SELECT id FROM gap_v")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT * FROM gap_vw").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["id".to_string()]);
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected view id rows, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE OR REPLACE VIEW gap_vw AS SELECT label FROM gap_v")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT * FROM gap_vw").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["label".to_string()]);
                assert_eq!(rows, vec![vec!["a".to_string()]]);
            }
            other => panic!("expected replaced view rows, got {other:?}"),
        }
        match client.query("SHOW CREATE VIEW gap_vw").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][1].contains("SELECT label FROM gap_v"),
                    "SHOW CREATE VIEW should use replaced SQL, got {}",
                    rows[0][1]
                );
            }
            other => panic!("expected SHOW CREATE VIEW rows, got {other:?}"),
        }

        match client
            .query("CREATE VIEW gap_vw AS SELECT id FROM gap_v")
            .await
        {
            QueryResponse::Err { message, .. } => {
                assert!(
                    message.contains("already exists") || message.contains("已存在"),
                    "plain CREATE VIEW must still error on duplicate, got {message}"
                );
            }
            other => panic!("expected duplicate CREATE VIEW error, got {other:?}"),
        }

        match client
            .query("CREATE OR REPLACE VIEW gap_v AS SELECT id FROM gap_v")
            .await
        {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1347);
                assert!(message.contains("VIEW"), "expected not VIEW, got {message}");
            }
            other => panic!("expected errno 1347 replacing a base table, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M125: text PREPARE / EXECUTE / DEALLOCATE PREPARE is session-scoped; COM_STMT_* unchanged.
    #[tokio::test]
    async fn prepare_execute_text_wire() {
        let server = TestServer::start("prepare_execute_text").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client.query("PREPARE gap_stmt FROM 'SELECT 1'").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("EXECUTE gap_stmt").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["1".to_string()]);
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("EXECUTE should match SELECT 1, got {other:?}"),
        }
        match client.query("SELECT 1").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["1".to_string()]);
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("SELECT 1 control, got {other:?}"),
        }

        assert!(matches!(
            client.query("DEALLOCATE PREPARE gap_stmt").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("EXECUTE gap_stmt").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1243);
                assert!(
                    message.contains("gap_stmt"),
                    "errno 1243 should name the statement, got {message}"
                );
            }
            other => panic!("expected errno 1243 after DEALLOCATE, got {other:?}"),
        }

        assert!(matches!(
            client.query("PREPARE gap_stmt FROM 'SELECT 1'").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("DROP PREPARE gap_stmt").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("EXECUTE gap_stmt").await,
            QueryResponse::Err { code: 1243, .. }
        ));

        match client.query("PREPARE gap_bad FROM 'NOT A STATEMENT'").await {
            QueryResponse::Err { code, .. } => {
                assert_eq!(code, 1064, "invalid SQL must fail at PREPARE");
            }
            other => panic!("expected parse error at PREPARE, got {other:?}"),
        }

        let stmt_id = client.stmt_prepare("SELECT 1").await;
        assert_eq!(stmt_id, 1);
        match client.stmt_execute(stmt_id, &[]).await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("binary COM_STMT_* must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn com_reset_connection_clears_text_prepare() {
        let server = TestServer::start("text_prepare_reset").await;
        let mut client = server.connect().await;
        assert!(matches!(
            client.query("PREPARE gap_stmt FROM 'SELECT 1'").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.reset_connection().await,
            QueryResponse::Ok { .. }
        ));
        match client.query("EXECUTE gap_stmt").await {
            QueryResponse::Err { code: 1243, .. } => {}
            other => panic!("RESET must clear text PREPARE, got {other:?}"),
        }
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

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

    /// M97: SHOW PROCEDURE STATUS lists catalog ProcedureMeta rows.
    #[tokio::test]
    async fn show_procedure_status_stubs() {
        let server = TestServer::start("show_procedure_status").await;
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
                .query("CREATE FUNCTION f() RETURNS INT BEGIN RETURN 42; END")
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

        match client.query("SHOW PROCEDURE STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Db");
                assert!(columns.contains(&"Name".to_string()));
                assert!(columns.contains(&"Type".to_string()));
                assert!(columns.contains(&"Definer".to_string()));
                assert!(columns.contains(&"character_set_client".to_string()));
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][1], "p");
                assert_eq!(rows[0][2], "PROCEDURE");
            }
            other => panic!("expected SHOW PROCEDURE STATUS rows, got {other:?}"),
        }
        match client.query("SHOW PROCEDURE STATUS LIKE 'p%'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][1], "p");
            }
            other => panic!("expected LIKE rows, got {other:?}"),
        }
        match client.query("SHOW PROCEDURE STATUS LIKE 'no_such%'").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE, got {other:?}"),
        }

        match client.query("SHOW CREATE FUNCTION f").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Function");
                assert!(rows[0][2].contains("CREATE FUNCTION `f`()"));
            }
            other => panic!("SHOW CREATE FUNCTION must stay unchanged, got {other:?}"),
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

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M98: SHOW FUNCTION STATUS lists catalog FunctionMeta rows.
    #[tokio::test]
    async fn show_function_status_stubs() {
        let server = TestServer::start("show_function_status").await;
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

        match client.query("SHOW FUNCTION STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Db");
                assert!(columns.contains(&"Name".to_string()));
                assert!(columns.contains(&"Type".to_string()));
                assert!(columns.contains(&"Definer".to_string()));
                assert!(columns.contains(&"character_set_client".to_string()));
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][1], "f");
                assert_eq!(rows[0][2], "FUNCTION");
            }
            other => panic!("expected SHOW FUNCTION STATUS rows, got {other:?}"),
        }
        match client.query("SHOW FUNCTION STATUS LIKE 'f%'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][1], "f");
            }
            other => panic!("expected LIKE rows, got {other:?}"),
        }
        match client.query("SHOW FUNCTION STATUS LIKE 'no_such%'").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE, got {other:?}"),
        }

        match client.query("SHOW PROCEDURE STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Db");
                assert_eq!(rows[0][1], "p");
                assert_eq!(rows[0][2], "PROCEDURE");
            }
            other => panic!("SHOW PROCEDURE STATUS must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW CREATE FUNCTION f").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Function");
                assert!(rows[0][2].contains("CREATE FUNCTION `f`()"));
            }
            other => panic!("SHOW CREATE FUNCTION must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW CREATE PROCEDURE p").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Procedure");
                assert!(rows[0][2].contains("CREATE PROCEDURE `p`()"));
            }
            other => panic!("SHOW CREATE PROCEDURE must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M99: SHOW CREATE USER reconstructs catalog account DDL without hashes.
    #[tokio::test]
    async fn show_create_user_stubs() {
        let server = TestServer::start("show_create_user").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
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

        match client.query("SHOW CREATE USER 'app'@'%'").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["CREATE USER for app@%".to_string()]);
                assert_eq!(rows.len(), 1);
                assert_eq!(
                    rows[0][0],
                    "CREATE USER `app`@`%` IDENTIFIED WITH 'mysql_native_password'"
                );
                assert!(!rows[0][0].contains("secret"));
                assert!(!rows[0][0].contains("BY "));
                assert!(!rows[0][0].contains("AS "));
            }
            other => panic!("expected SHOW CREATE USER rows, got {other:?}"),
        }

        match client.query("SHOW CREATE USER 'no_such'@'%'").await {
            QueryResponse::Err { code: 3162, .. } => {}
            other => panic!("expected errno 3162 for unknown user, got {other:?}"),
        }

        match client.query("SHOW FUNCTION STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Db");
                assert_eq!(rows[0][1], "f");
                assert_eq!(rows[0][2], "FUNCTION");
            }
            other => panic!("SHOW FUNCTION STATUS must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW PROCEDURE STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Db");
                assert_eq!(rows[0][1], "p");
                assert_eq!(rows[0][2], "PROCEDURE");
            }
            other => panic!("SHOW PROCEDURE STATUS must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW CREATE FUNCTION f").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Function");
                assert!(rows[0][2].contains("CREATE FUNCTION `f`()"));
            }
            other => panic!("SHOW CREATE FUNCTION must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M100: SHOW CREATE EVENT is accepted and returns missing-event errno 1539.
    #[tokio::test]
    async fn show_create_event_stubs() {
        let server = TestServer::start("show_create_event").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
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

        match client.query("SHOW CREATE EVENT e").await {
            QueryResponse::Err {
                code: 1539,
                message,
            } => {
                assert!(message.contains("e"));
            }
            other => panic!("expected errno 1539 for unknown event, got {other:?}"),
        }
        match client.query("SHOW CREATE EVENT rusql.no_such").await {
            QueryResponse::Err { code: 1539, .. } => {}
            other => panic!("expected errno 1539 for qualified unknown event, got {other:?}"),
        }

        match client.query("SHOW CREATE USER 'app'@'%'").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["CREATE USER for app@%".to_string()]);
                assert_eq!(
                    rows[0][0],
                    "CREATE USER `app`@`%` IDENTIFIED WITH 'mysql_native_password'"
                );
            }
            other => panic!("SHOW CREATE USER must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW FUNCTION STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Db");
                assert_eq!(rows[0][1], "f");
                assert_eq!(rows[0][2], "FUNCTION");
            }
            other => panic!("SHOW FUNCTION STATUS must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW PROCEDURE STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Db");
                assert_eq!(rows[0][1], "p");
                assert_eq!(rows[0][2], "PROCEDURE");
            }
            other => panic!("SHOW PROCEDURE STATUS must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M101: SHOW EVENTS returns MySQL-shaped columns over an empty catalog.
    #[tokio::test]
    async fn show_events_stubs() {
        let server = TestServer::start("show_events").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE FUNCTION f() RETURNS INT BEGIN RETURN 42; END")
                .await,
            QueryResponse::Ok { .. }
        ));

        match client.query("SHOW EVENTS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Db");
                assert!(columns.contains(&"Name".to_string()));
                assert!(columns.contains(&"Status".to_string()));
                assert!(columns.contains(&"character_set_client".to_string()));
                assert!(rows.is_empty());
            }
            other => panic!("expected empty SHOW EVENTS rows, got {other:?}"),
        }
        match client.query("SHOW EVENTS LIKE 'e%'").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE, got {other:?}"),
        }
        match client.query("SHOW EVENTS FROM no_such_db").await {
            QueryResponse::Err { code: 1049, .. } => {}
            other => panic!("expected errno 1049 for unknown db, got {other:?}"),
        }

        match client.query("SHOW CREATE EVENT e").await {
            QueryResponse::Err { code: 1539, .. } => {}
            other => panic!("SHOW CREATE EVENT must stay errno 1539, got {other:?}"),
        }
        match client.query("SHOW CREATE USER 'app'@'%'").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["CREATE USER for app@%".to_string()]);
                assert_eq!(
                    rows[0][0],
                    "CREATE USER `app`@`%` IDENTIFIED WITH 'mysql_native_password'"
                );
            }
            other => panic!("SHOW CREATE USER must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW FUNCTION STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Db");
                assert_eq!(rows[0][1], "f");
                assert_eq!(rows[0][2], "FUNCTION");
            }
            other => panic!("SHOW FUNCTION STATUS must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M102: CREATE EVENT catalog feeds SHOW EVENTS / SHOW CREATE EVENT.
    #[tokio::test]
    async fn create_event_catalog() {
        let server = TestServer::start("create_event").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'")
                .await,
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
                .query("CREATE EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' DO SELECT 1")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client
            .query("CREATE EVENT e ON SCHEDULE EVERY 1 HOUR DO SELECT 1")
            .await
        {
            QueryResponse::Err { code: 1537, .. } => {}
            other => panic!("expected errno 1537 for duplicate event, got {other:?}"),
        }

        match client.query("SHOW EVENTS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Db");
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][1], "e");
                assert_eq!(rows[0][4], "ONE TIME");
            }
            other => panic!("expected SHOW EVENTS catalog row, got {other:?}"),
        }
        match client.query("SHOW EVENTS LIKE 'e%'").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows.len(), 1),
            other => panic!("expected LIKE match, got {other:?}"),
        }
        match client.query("SHOW CREATE EVENT e").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Event");
                assert!(rows[0][3].contains("DEFINER=`root`@`%`"));
                assert!(rows[0][3].contains("EVENT `e` ON SCHEDULE AT"));
                assert!(rows[0][3].contains("ON COMPLETION NOT PRESERVE"));
            }
            other => panic!("expected reconstructed SHOW CREATE EVENT, got {other:?}"),
        }

        assert!(matches!(
            client.query("DROP EVENT e").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE EVENT e").await {
            QueryResponse::Err { code: 1539, .. } => {}
            other => panic!("expected errno 1539 after DROP EVENT, got {other:?}"),
        }

        match client.query("SHOW CREATE USER 'app'@'%'").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["CREATE USER for app@%".to_string()]);
                assert_eq!(
                    rows[0][0],
                    "CREATE USER `app`@`%` IDENTIFIED WITH 'mysql_native_password'"
                );
            }
            other => panic!("SHOW CREATE USER must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW FUNCTION STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Db");
                assert_eq!(rows[0][1], "f");
                assert_eq!(rows[0][2], "FUNCTION");
            }
            other => panic!("SHOW FUNCTION STATUS must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M103: ALTER EVENT updates catalog schedule / status / name / body.
    #[tokio::test]
    async fn alter_event_catalog() {
        let server = TestServer::start("alter_event").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'")
                .await,
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
                .query("CREATE EVENT e ON SCHEDULE EVERY 1 HOUR DO SELECT 1")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("ALTER EVENT e ON SCHEDULE EVERY 1 DAY").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("ALTER EVENT e DISABLE").await,
            QueryResponse::Ok { .. }
        ));

        match client.query("SHOW EVENTS LIKE 'e'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][4], "RECURRING");
                assert_eq!(rows[0][7], "DAY");
                assert_eq!(rows[0][10], "DISABLED");
            }
            other => panic!("expected updated SHOW EVENTS row, got {other:?}"),
        }
        match client.query("SHOW CREATE EVENT e").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows[0][3].contains("EVERY 1 DAY"));
            }
            other => panic!("expected updated SHOW CREATE EVENT, got {other:?}"),
        }
        match client.query("ALTER EVENT no_such ENABLE").await {
            QueryResponse::Err { code: 1539, .. } => {}
            other => panic!("expected errno 1539 for unknown ALTER EVENT, got {other:?}"),
        }

        match client.query("SHOW CREATE USER 'app'@'%'").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["CREATE USER for app@%".to_string()]);
                assert_eq!(
                    rows[0][0],
                    "CREATE USER `app`@`%` IDENTIFIED WITH 'mysql_native_password'"
                );
            }
            other => panic!("SHOW CREATE USER must stay unchanged, got {other:?}"),
        }
        match client.query("SHOW FUNCTION STATUS").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Db");
                assert_eq!(rows[0][1], "f");
                assert_eq!(rows[0][2], "FUNCTION");
            }
            other => panic!("SHOW FUNCTION STATUS must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M104: due ONE TIME AT events run DO; @@event_scheduler is ON / read-only.
    #[tokio::test]
    async fn event_scheduler_due_at() {
        let server = TestServer::start("event_scheduler").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("CREATE TABLE t (id INT PRIMARY KEY)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE EVENT due_e ON SCHEDULE AT '2000-01-01 00:00:00' DO INSERT INTO t VALUES (1)")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE EVENT future_e ON SCHEDULE AT '2038-01-01 00:00:00' DO INSERT INTO t VALUES (2)")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE EVENT rec_e ON SCHEDULE EVERY 1 HOUR DO INSERT INTO t VALUES (3)")
                .await,
            QueryResponse::Ok { .. }
        ));

        match client.query("SELECT id FROM t ORDER BY id").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["1".to_string()], vec!["3".to_string()]],
                    "due AT inserts 1; first EVERY fire inserts 3"
                );
            }
            other => panic!("expected due AT + first EVERY inserts, got {other:?}"),
        }
        match client.query("SHOW EVENTS LIKE 'due_e'").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows.is_empty(), "due event should be dropped after run");
            }
            other => panic!("expected empty SHOW EVENTS for due_e, got {other:?}"),
        }
        match client.query("SHOW EVENTS LIKE 'future_e'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
            }
            other => panic!("expected future_e to remain, got {other:?}"),
        }
        match client.query("SHOW EVENTS LIKE 'rec_e'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
            }
            other => panic!("expected rec_e to remain, got {other:?}"),
        }

        match client.query("SELECT @@event_scheduler").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["ON".to_string()]]);
            }
            other => panic!("expected @@event_scheduler ON, got {other:?}"),
        }
        match client.query("SHOW VARIABLES LIKE 'event_scheduler'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["event_scheduler".to_string(), "ON".to_string()]]
                );
            }
            other => panic!("expected SHOW VARIABLES event_scheduler, got {other:?}"),
        }
        match client.query("SET @@event_scheduler = 'OFF'").await {
            QueryResponse::Err { code: 1238, .. } => {}
            other => panic!("expected errno 1238 for SET event_scheduler, got {other:?}"),
        }
        match client.query("SET GLOBAL event_scheduler = ON").await {
            QueryResponse::Err { code: 1229, .. } => {}
            other => panic!("expected errno 1229 for SET GLOBAL event_scheduler, got {other:?}"),
        }

        assert!(matches!(
            client.query("ALTER EVENT future_e DISABLE").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE USER 'app'@'%'").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["CREATE USER for app@%".to_string()]);
                assert_eq!(
                    rows[0][0],
                    "CREATE USER `app`@`%` IDENTIFIED WITH 'mysql_native_password'"
                );
            }
            other => panic!("SHOW CREATE USER must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M105: ENABLED EVERY events run DO on next COM_QUERY and stay in the catalog.
    #[tokio::test]
    async fn event_scheduler_every_interval() {
        let server = TestServer::start("event_scheduler_every").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("CREATE TABLE t (id INT)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE EVENT rec_e ON SCHEDULE EVERY 1 HOUR DO INSERT INTO t VALUES (1)")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT id FROM t").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected first EVERY fire to insert 1, got {other:?}"),
        }
        match client.query("SELECT id FROM t").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["1".to_string()]],
                    "EVERY 1 HOUR must not re-fire on the next statement in the same hour"
                );
            }
            other => panic!("expected no second EVERY fire, got {other:?}"),
        }
        match client.query("SHOW EVENTS LIKE 'rec_e'").await {
            QueryResponse::Rows { columns, rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(
                    columns.len(),
                    15,
                    "SHOW EVENTS must not gain a last-executed column"
                );
            }
            other => panic!("expected rec_e to remain, got {other:?}"),
        }
        match client.query("SHOW CREATE USER 'app'@'%'").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["CREATE USER for app@%".to_string()]);
                assert_eq!(
                    rows[0][0],
                    "CREATE USER `app`@`%` IDENTIFIED WITH 'mysql_native_password'"
                );
            }
            other => panic!("SHOW CREATE USER must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M106: STARTS/ENDS gate EVERY; SHOW EVENTS/CREATE reconstruct the window.
    #[tokio::test]
    async fn event_scheduler_starts_ends() {
        let server = TestServer::start("event_scheduler_starts_ends").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("CREATE TABLE t (id INT)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE EVENT future_s ON SCHEDULE EVERY 1 HOUR STARTS '2038-01-01 00:00:00' DO INSERT INTO t VALUES (1)")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE EVENT past_e ON SCHEDULE EVERY 1 HOUR STARTS '2000-01-01 00:00:00' ENDS '2000-01-02 00:00:00' DO INSERT INTO t VALUES (2)")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE EVENT in_win ON SCHEDULE EVERY 1 HOUR STARTS '2000-01-01 00:00:00' ENDS '2038-01-01 00:00:00' DO INSERT INTO t VALUES (3)")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT id FROM t").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["3".to_string()]],
                    "only the in-window EVERY event should fire"
                );
            }
            other => panic!("expected in-window insert, got {other:?}"),
        }
        match client.query("SHOW EVENTS LIKE 'in_win'").await {
            QueryResponse::Rows { columns, rows, .. } => {
                assert_eq!(columns.len(), 15);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][8], "2000-01-01 00:00:00");
                assert_eq!(rows[0][9], "2038-01-01 00:00:00");
            }
            other => panic!("expected Starts/Ends from catalog, got {other:?}"),
        }
        match client.query("SHOW CREATE EVENT in_win").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][3].contains("STARTS '2000-01-01 00:00:00'"),
                    "SHOW CREATE EVENT must reconstruct STARTS, got {:?}",
                    rows[0][3]
                );
                assert!(
                    rows[0][3].contains("ENDS '2038-01-01 00:00:00'"),
                    "SHOW CREATE EVENT must reconstruct ENDS, got {:?}",
                    rows[0][3]
                );
            }
            other => panic!("expected reconstructed STARTS/ENDS, got {other:?}"),
        }
        assert!(matches!(
            client
                .query("ALTER EVENT future_s STARTS '2000-01-01 00:00:00'")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT id FROM t").await {
            QueryResponse::Rows { rows, .. } => {
                let mut got = rows;
                got.sort();
                assert_eq!(
                    got,
                    vec![vec!["1".to_string()], vec!["3".to_string()]],
                    "ALTER STARTS to the past should allow the next COM_QUERY fire"
                );
            }
            other => panic!("expected ALTER STARTS fire, got {other:?}"),
        }
        match client.query("SHOW CREATE USER 'app'@'%'").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["CREATE USER for app@%".to_string()]);
                assert_eq!(
                    rows[0][0],
                    "CREATE USER `app`@`%` IDENTIFIED WITH 'mysql_native_password'"
                );
            }
            other => panic!("SHOW CREATE USER must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M107: DEFINER / ON COMPLETION persist; PRESERVE keeps AT events DISABLED after run.
    #[tokio::test]
    async fn event_scheduler_definer_on_completion() {
        let server = TestServer::start("event_scheduler_definer").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("CREATE TABLE t (id INT)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE DEFINER=`app`@`%` EVENT keep_e ON SCHEDULE AT '2000-01-01 00:00:00' ON COMPLETION PRESERVE DO INSERT INTO t VALUES (1)")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT id FROM t").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected PRESERVE AT insert, got {other:?}"),
        }
        match client.query("SHOW EVENTS LIKE 'keep_e'").await {
            QueryResponse::Rows { columns, rows, .. } => {
                assert_eq!(columns.len(), 15);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][2], "app@%");
                assert_eq!(rows[0][10], "DISABLED");
            }
            other => panic!("expected PRESERVE AT to remain DISABLED, got {other:?}"),
        }
        match client.query("SHOW CREATE EVENT keep_e").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][3].contains("DEFINER=`app`@`%`"),
                    "SHOW CREATE EVENT must reconstruct DEFINER, got {:?}",
                    rows[0][3]
                );
                assert!(
                    rows[0][3].contains("ON COMPLETION PRESERVE"),
                    "SHOW CREATE EVENT must reconstruct ON COMPLETION, got {:?}",
                    rows[0][3]
                );
            }
            other => panic!("expected reconstructed DEFINER/ON COMPLETION, got {other:?}"),
        }
        match client.query("SHOW CREATE USER 'app'@'%'").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["CREATE USER for app@%".to_string()]);
                assert_eq!(
                    rows[0][0],
                    "CREATE USER `app`@`%` IDENTIFIED WITH 'mysql_native_password'"
                );
            }
            other => panic!("SHOW CREATE USER must stay unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M108: Event COMMENT persists; SHOW CREATE EVENT reconstructs it; SHOW EVENTS stays 15 columns.
    #[tokio::test]
    async fn create_event_comment_persists() {
        let server = TestServer::start("create_event_comment").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE EVENT ec_e ON SCHEDULE EVERY 1 HOUR COMMENT 'hi' DO SELECT 1")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE EVENT ec_e").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][3].contains("COMMENT 'hi'"),
                    "SHOW CREATE EVENT must reconstruct COMMENT, got {:?}",
                    rows[0][3]
                );
                assert!(
                    rows[0][3].contains("DEFINER=`root`@`%`"),
                    "M107 DEFINER reconstruction must stay, got {:?}",
                    rows[0][3]
                );
                assert!(
                    rows[0][3].contains("ON COMPLETION NOT PRESERVE"),
                    "M107 ON COMPLETION reconstruction must stay, got {:?}",
                    rows[0][3]
                );
            }
            other => panic!("expected reconstructed COMMENT, got {other:?}"),
        }
        match client.query("SHOW EVENTS LIKE 'ec_e'").await {
            QueryResponse::Rows { columns, rows, .. } => {
                assert_eq!(columns.len(), 15);
                assert_eq!(rows.len(), 1);
            }
            other => panic!("SHOW EVENTS must stay 15 columns, got {other:?}"),
        }

        assert!(matches!(
            client.query("ALTER EVENT ec_e COMMENT 'bye'").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE EVENT ec_e").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][3].contains("COMMENT 'bye'"),
                    "ALTER EVENT COMMENT must update SHOW CREATE EVENT, got {:?}",
                    rows[0][3]
                );
                assert!(
                    !rows[0][3].contains("COMMENT 'hi'"),
                    "old COMMENT must be replaced, got {:?}",
                    rows[0][3]
                );
            }
            other => panic!("expected updated COMMENT, got {other:?}"),
        }
        match client.query("SHOW CREATE USER 'app'@'%'").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["CREATE USER for app@%".to_string()]);
                assert_eq!(
                    rows[0][0],
                    "CREATE USER `app`@`%` IDENTIFIED WITH 'mysql_native_password'"
                );
            }
            other => panic!("SHOW CREATE USER must stay unchanged, got {other:?}"),
        }

        assert!(matches!(
            client.query("DROP EVENT ec_e").await,
            QueryResponse::Ok { .. }
        ));

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M130: CREATE EVENT … DISABLE ON SLAVE persists; SHOW CREATE reconstructs; scheduler skips.
    #[tokio::test]
    async fn create_event_disable_on_slave() {
        let server = TestServer::start("create_event_disable_on_slave").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE EVENT e_dos ON SCHEDULE EVERY 1 HOUR DISABLE ON SLAVE DO SELECT 1")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE EVENT e_dos").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][3].contains("DISABLE ON SLAVE"),
                    "SHOW CREATE EVENT must reconstruct DISABLE ON SLAVE, got {:?}",
                    rows[0][3]
                );
                assert!(
                    rows[0][3].contains("DEFINER=`root`@`%`"),
                    "M107 DEFINER reconstruction must stay, got {:?}",
                    rows[0][3]
                );
                assert!(
                    !rows[0][3].contains("COMMENT"),
                    "DISABLE ON SLAVE must not invent COMMENT, got {:?}",
                    rows[0][3]
                );
            }
            other => panic!("expected reconstructed DISABLE ON SLAVE, got {other:?}"),
        }
        match client.query("SHOW EVENTS LIKE 'e_dos'").await {
            QueryResponse::Rows { columns, rows, .. } => {
                assert_eq!(columns.len(), 15);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][10], "SLAVESIDE_DISABLED");
            }
            other => panic!("SHOW EVENTS must list slave-disabled events, got {other:?}"),
        }

        assert!(matches!(
            client.query("ALTER EVENT e_dos ENABLE").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW CREATE EVENT e_dos").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    !rows[0][3].contains("DISABLE ON SLAVE"),
                    "ALTER EVENT ENABLE must clear DISABLE ON SLAVE, got {:?}",
                    rows[0][3]
                );
            }
            other => panic!("expected ENABLE to clear slave-disable, got {other:?}"),
        }
        match client.query("SHOW EVENTS LIKE 'e_dos'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows[0][10], "ENABLED");
            }
            other => panic!("ENABLE without ON SLAVE must stay ENABLED, got {other:?}"),
        }

        assert!(matches!(
            client.query("ALTER EVENT e_dos DISABLE").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SHOW EVENTS LIKE 'e_dos'").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows[0][10], "DISABLED");
            }
            other => panic!("DISABLE without ON SLAVE must stay DISABLED, got {other:?}"),
        }
        match client.query("SHOW CREATE EVENT e_dos").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    !rows[0][3].contains("DISABLE ON SLAVE"),
                    "plain DISABLE must not reconstruct DISABLE ON SLAVE, got {:?}",
                    rows[0][3]
                );
            }
            other => panic!("expected plain DISABLE reconstruction, got {other:?}"),
        }

        assert!(matches!(
            client.query("DROP EVENT e_dos").await,
            QueryResponse::Ok { .. }
        ));

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M109: information_schema.EVENTS lists catalog events; SHOW EVENTS stays 15 columns.
    #[tokio::test]
    async fn information_schema_events() {
        let server = TestServer::start("information_schema_events").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE EVENT ise_e ON SCHEDULE EVERY 1 HOUR COMMENT 'hi' DO SELECT 1")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client
            .query("SELECT EVENT_NAME, EVENT_COMMENT, LAST_EXECUTED FROM information_schema.EVENTS")
            .await
        {
            QueryResponse::Rows { columns, rows, .. } => {
                let name_i = columns.iter().position(|c| c == "EVENT_NAME").unwrap();
                let comment_i = columns.iter().position(|c| c == "EVENT_COMMENT").unwrap();
                let last_i = columns.iter().position(|c| c == "LAST_EXECUTED").unwrap();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0][name_i], "ise_e");
                assert_eq!(rows[0][comment_i], "hi");
                // EVERY events may fire on this COM_QUERY (M105); never invent a timestamp.
                let last = &rows[0][last_i];
                assert!(
                    last.is_empty() || last.starts_with("20"),
                    "LAST_EXECUTED must be empty or a stored watermark, got {last:?}"
                );
            }
            other => panic!("expected information_schema.EVENTS row, got {other:?}"),
        }
        match client
            .query("SELECT EVENT_NAME FROM information_schema.events")
            .await
        {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows.len(), 1),
            other => panic!("lowercase information_schema.events must work, got {other:?}"),
        }
        match client.query("SHOW EVENTS LIKE 'ise_e'").await {
            QueryResponse::Rows { columns, rows, .. } => {
                assert_eq!(columns.len(), 15);
                assert_eq!(rows.len(), 1);
            }
            other => panic!("SHOW EVENTS must stay 15 columns, got {other:?}"),
        }
        match client.query("SHOW CREATE EVENT ise_e").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][3].contains("DEFINER=`root`@`%`"),
                    "M107 DEFINER reconstruction must stay, got {:?}",
                    rows[0][3]
                );
                assert!(
                    rows[0][3].contains("ON COMPLETION NOT PRESERVE"),
                    "M107 ON COMPLETION reconstruction must stay, got {:?}",
                    rows[0][3]
                );
                assert!(
                    rows[0][3].contains("COMMENT 'hi'"),
                    "M108 COMMENT reconstruction must stay, got {:?}",
                    rows[0][3]
                );
            }
            other => panic!("expected SHOW CREATE EVENT, got {other:?}"),
        }

        assert!(matches!(
            client.query("DROP EVENT ise_e").await,
            QueryResponse::Ok { .. }
        ));

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M119: information_schema.TABLE_CONSTRAINTS lists PK / UNIQUE / FK; unknown I_S still 1146.
    #[tokio::test]
    async fn table_constraints() {
        let server = TestServer::start("table_constraints").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE TABLE gap_tc (id INT PRIMARY KEY)")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client
            .query(
                "SELECT CONSTRAINT_NAME FROM information_schema.TABLE_CONSTRAINTS WHERE TABLE_NAME = 'gap_tc'",
            )
            .await
        {
            QueryResponse::Rows { columns, rows, .. } => {
                assert_eq!(columns, vec!["CONSTRAINT_NAME".to_string()]);
                assert_eq!(rows, vec![vec!["PRIMARY".to_string()]]);
            }
            other => panic!("expected PRIMARY CONSTRAINT_NAME, got {other:?}"),
        }
        match client
            .query(
                "SELECT CONSTRAINT_NAME FROM information_schema.table_constraints WHERE TABLE_NAME = 'gap_tc'",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["PRIMARY".to_string()]]);
            }
            other => panic!("lowercase table_constraints must work, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE TABLE tc_uq (id INT PRIMARY KEY, name VARCHAR(8) UNIQUE)")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client
            .query(
                "SELECT CONSTRAINT_NAME, CONSTRAINT_TYPE FROM information_schema.TABLE_CONSTRAINTS WHERE TABLE_NAME = 'tc_uq' ORDER BY CONSTRAINT_TYPE, CONSTRAINT_NAME",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows.iter()
                        .any(|r| r[0] == "PRIMARY" && r[1] == "PRIMARY KEY"),
                    "missing PK: {rows:?}"
                );
                assert!(
                    rows.iter().any(|r| r[0] == "name" && r[1] == "UNIQUE"),
                    "missing UNIQUE: {rows:?}"
                );
                assert!(rows.iter().all(|r| r[1] != "CHECK"));
            }
            other => panic!("expected UNIQUE + PK, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE TABLE tc_parent (id INT PRIMARY KEY)")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query(
                    "CREATE TABLE tc_child (id INT PRIMARY KEY, parent_id INT, CONSTRAINT fk_tc_child FOREIGN KEY (parent_id) REFERENCES tc_parent (id))",
                )
                .await,
            QueryResponse::Ok { .. }
        ));
        match client
            .query(
                "SELECT CONSTRAINT_NAME, CONSTRAINT_TYPE FROM information_schema.TABLE_CONSTRAINTS WHERE TABLE_NAME = 'tc_child' ORDER BY CONSTRAINT_TYPE, CONSTRAINT_NAME",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows.iter()
                        .any(|r| r[0] == "fk_tc_child" && r[1] == "FOREIGN KEY"),
                    "missing FK: {rows:?}"
                );
            }
            other => panic!("expected FOREIGN KEY row, got {other:?}"),
        }
        match client
            .query(
                "SELECT CONSTRAINT_NAME FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_NAME = 'tc_child'",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows.iter().any(|r| r.iter().any(|c| c == "fk_tc_child")),
                    "KEY_COLUMN_USAGE must still list the FK, got {rows:?}"
                );
            }
            other => panic!("KEY_COLUMN_USAGE unchanged, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("SELECT * FROM information_schema.NO_SUCH_IS_TABLE")
                .await,
            QueryResponse::Err { code: 1146, .. }
        ));

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M156: information_schema.COLUMNS exposes COLUMN_DEFAULT, EXTRA, COLUMN_KEY.
    #[tokio::test]
    async fn columns_extra() {
        let server = TestServer::start("columns_extra").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query(
                    "CREATE TABLE ce_t (id INT NOT NULL AUTO_INCREMENT PRIMARY KEY, tag VARCHAR(8) DEFAULT 'x', note VARCHAR(8))",
                )
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE INDEX idx_ce_note ON ce_t (note)")
                .await,
            QueryResponse::Ok { .. }
        ));

        match client
            .query(
                "SELECT COLUMN_NAME, COLUMN_KEY, EXTRA, COLUMN_DEFAULT FROM information_schema.COLUMNS WHERE TABLE_NAME = 'ce_t'",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                let row = |name: &str| {
                    rows.iter()
                        .find(|r| r[0] == name)
                        .unwrap_or_else(|| panic!("missing column {name} in {rows:?}"))
                };
                let id = row("id");
                assert_eq!(id[1], "PRI");
                assert!(id[2].contains("auto_increment"));
                assert!(id[3].is_empty());
                let tag = row("tag");
                assert_eq!(tag[1], "");
                assert_eq!(tag[2], "");
                assert_eq!(tag[3], "x");
                let note = row("note");
                assert_eq!(note[1], "MUL");
                assert_eq!(note[2], "");
                assert!(note[3].is_empty());
            }
            other => panic!("expected COLUMNS extras rows, got {other:?}"),
        }

        match client
            .query("SELECT * FROM information_schema.columns WHERE table_schema = 'no_such_db'")
            .await
        {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            QueryResponse::Err { code: 1146, .. } => {
                panic!("unknown schema must not errno 1146 for COLUMNS")
            }
            other => panic!("expected empty COLUMNS for unknown schema, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M120: information_schema.PROCESSLIST lists live sessions; SHOW columns stay M53.
    #[tokio::test]
    async fn information_schema_processlist() {
        let server = TestServer::start("information_schema_processlist").await;
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
        assert_ne!(id_a, id_b);

        match a
            .query("SELECT ID FROM information_schema.PROCESSLIST")
            .await
        {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["ID".to_string()]);
                assert!(
                    rows.iter().any(|r| r[0] == id_a),
                    "PROCESSLIST must include connection A {id_a}, got {rows:?}"
                );
                assert!(
                    rows.iter().any(|r| r[0] == id_b),
                    "PROCESSLIST must include connection B {id_b}, got {rows:?}"
                );
            }
            other => panic!("expected PROCESSLIST ID rows, got {other:?}"),
        }

        match a
            .query("SELECT ID FROM information_schema.processlist")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows.iter().any(|r| r[0] == id_a));
            }
            other => panic!("lowercase processlist must work, got {other:?}"),
        }

        match a.query("SHOW PROCESSLIST").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns[0], "Id");
                assert_eq!(columns.len(), 8);
                assert!(rows.iter().any(|r| r[0] == id_a));
                assert!(rows.iter().any(|r| r[0] == id_b));
            }
            other => panic!("SHOW PROCESSLIST columns must stay M53, got {other:?}"),
        }

        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M121: information_schema.PARAMETERS is queryable (empty until M132); unknown I_S stays 1146.
    #[tokio::test]
    async fn information_schema_parameters() {
        let server = TestServer::start("information_schema_parameters").await;
        let mut client = server.connect().await;

        match client
            .query("SELECT SPECIFIC_NAME FROM information_schema.PARAMETERS LIMIT 1")
            .await
        {
            QueryResponse::Rows { columns, rows, .. } => {
                assert_eq!(columns, vec!["SPECIFIC_NAME".to_string()]);
                assert!(
                    rows.is_empty(),
                    "empty PARAMETERS catalog must be 0 rows, got {rows:?}"
                );
            }
            other => panic!("expected PARAMETERS rows, got {other:?}"),
        }

        match client
            .query("SELECT SPECIFIC_NAME FROM information_schema.parameters LIMIT 1")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows.is_empty());
            }
            other => panic!("lowercase parameters must work, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE PROCEDURE gap_param() BEGIN SELECT 1; END")
                .await,
            QueryResponse::Ok { .. }
        ));

        match client
            .query(
                "SELECT SPECIFIC_NAME, PARAMETER_MODE, PARAMETER_NAME, DATA_TYPE FROM information_schema.PARAMETERS LIMIT 1",
            )
            .await
        {
            QueryResponse::Rows { columns, rows, .. } => {
                assert_eq!(
                    columns,
                    vec![
                        "SPECIFIC_NAME".to_string(),
                        "PARAMETER_MODE".to_string(),
                        "PARAMETER_NAME".to_string(),
                        "DATA_TYPE".to_string(),
                    ]
                );
                assert!(
                    rows.is_empty(),
                    "zero-arg CREATE PROCEDURE must not invent parameters, got {rows:?}"
                );
            }
            other => panic!("expected projected PARAMETERS columns, got {other:?}"),
        }

        match client.query("SHOW CREATE PROCEDURE gap_param").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][2].contains("CREATE PROCEDURE `gap_param`()"),
                    "SHOW CREATE PROCEDURE empty list for zero-arg procedure, got {}",
                    rows[0][2]
                );
            }
            other => panic!("SHOW CREATE PROCEDURE must stay M95 empty params, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("SELECT * FROM information_schema.NO_SUCH_IS_TABLE")
                .await,
            QueryResponse::Err { code: 1146, .. }
        ));

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M132: CREATE PROCEDURE IN params, CALL with matching args, SHOW CREATE lists IN x INT.
    #[tokio::test]
    async fn procedure_in_param_call() {
        let server = TestServer::start("procedure_in_param").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE PROCEDURE gap_p(IN x INT) BEGIN SELECT x; END")
                .await,
            QueryResponse::Ok { .. }
        ));

        match client.query("CALL gap_p(3)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["3".to_string()]]);
            }
            other => panic!("CALL gap_p(3) must return 3, got {other:?}"),
        }

        match client.query("SHOW CREATE PROCEDURE gap_p").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][2].contains("IN x INT"),
                    "SHOW CREATE PROCEDURE must include IN x INT, got {}",
                    rows[0][2]
                );
                assert!(!rows[0][2].contains("CREATE PROCEDURE `gap_p`()"));
            }
            other => panic!("expected SHOW CREATE PROCEDURE rows, got {other:?}"),
        }

        match client
            .query(
                "SELECT PARAMETER_NAME, PARAMETER_MODE, DATA_TYPE FROM information_schema.PARAMETERS WHERE SPECIFIC_NAME = 'gap_p'",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec!["x".to_string(), "IN".to_string(), "INT".to_string()]]
                );
            }
            other => panic!("expected PARAMETERS row for IN x INT, got {other:?}"),
        }

        match client.query("CALL gap_p()").await {
            QueryResponse::Err { code: 1318, .. } => {}
            other => panic!("expected errno 1318 for wrong CALL arity, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE PROCEDURE gap_zero() BEGIN SELECT 1; END")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("CALL gap_zero()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("zero-arg CALL must still work, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M172: client `DELIMITER` batches create procedures without aborting the session.
    #[tokio::test]
    async fn delimiter_stub() {
        let server = TestServer::start("delimiter_stub").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client.query("DELIMITER //").await,
            QueryResponse::Ok { .. }
        ));

        let create_script = concat!(
            "DELIMITER //\n",
            "CREATE PROCEDURE sp_m172() BEGIN SELECT 42 AS n; END//\n",
            "DELIMITER ;"
        );
        assert!(matches!(
            client.query(create_script).await,
            QueryResponse::Ok { .. }
        ));
        match client.query("CALL sp_m172()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["42".to_string()]]);
            }
            other => panic!("delimiter script CALL must return 42, got {other:?}"),
        }

        assert!(matches!(
            client.query("SELECT 1").await,
            QueryResponse::Rows { .. }
        ));

        let semi_in_string = concat!(
            "DELIMITER //\n",
            "CREATE PROCEDURE sp_m172_q() BEGIN SELECT ';' AS d; END//\n",
            "DELIMITER ;"
        );
        assert!(matches!(
            client.query(semi_in_string).await,
            QueryResponse::Ok { .. }
        ));
        match client.query("CALL sp_m172_q()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec![";".to_string()]]);
            }
            other => panic!("delimiter with quoted semicolon failed: {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M165: OUT / INOUT procedure parameters assign user variables on CALL.
    #[tokio::test]
    async fn procedure_out_inout_call() {
        let server = TestServer::start("procedure_out_inout").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE PROCEDURE gap_out(OUT x INT) BEGIN SET x = 3; END")
                .await,
            QueryResponse::Ok { .. }
        ));

        assert!(matches!(
            client.query("CALL gap_out(@a)").await,
            QueryResponse::Ok { .. }
        ));

        match client.query("SELECT @a").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["3".to_string()]]);
            }
            other => panic!("SELECT @a after OUT CALL must be 3, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE PROCEDURE gap_io(INOUT x INT) BEGIN SET x = x + 1; END")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("SET @b = 5").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("CALL gap_io(@b)").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT @b").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["6".to_string()]]);
            }
            other => panic!("INOUT must write back incremented @b, got {other:?}"),
        }

        match client.query("CALL gap_out(3)").await {
            QueryResponse::Err { code: 1414, .. } => {}
            other => panic!("literal OUT arg must be errno 1414, got {other:?}"),
        }

        match client.query("SHOW CREATE PROCEDURE gap_out").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(
                    rows[0][2].contains("OUT x INT"),
                    "SHOW CREATE must list OUT x INT, got {}",
                    rows[0][2]
                );
            }
            other => panic!("expected SHOW CREATE PROCEDURE rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M168: IF / WHILE / LOOP / LEAVE in procedure bodies.
    #[tokio::test]
    async fn procedure_control_flow() {
        let server = TestServer::start("procedure_control_flow").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE PROCEDURE gap_if() BEGIN DECLARE n INT DEFAULT 1; IF n > 0 THEN SELECT 1; ELSE SELECT 0; END IF; END")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("CALL gap_if()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("CALL gap_if expected 1, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE PROCEDURE gap_wh() BEGIN DECLARE n INT DEFAULT 2; WHILE n > 0 DO SET n = n - 1; END WHILE; SELECT n; END")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("CALL gap_wh()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["0".to_string()]]);
            }
            other => panic!("CALL gap_wh expected 0, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE PROCEDURE gap_lp() BEGIN DECLARE n INT DEFAULT 0; outer_l: LOOP SET n = n + 1; IF n >= 2 THEN LEAVE outer_l; END IF; END LOOP; SELECT n; END")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("CALL gap_lp()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["2".to_string()]]);
            }
            other => panic!("CALL gap_lp expected 2, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M167: DECLARE locals in procedure bodies (DEFAULT, SET, SELECT).
    #[tokio::test]
    async fn procedure_declare_var() {
        let server = TestServer::start("procedure_declare_var").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE PROCEDURE gap_decl() BEGIN DECLARE n INT DEFAULT 1; SET n = n + 1; SELECT n; END")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("CALL gap_decl()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["2".to_string()]]);
            }
            other => panic!("CALL gap_decl must return 2, got {other:?}"),
        }

        assert!(matches!(
            client.query("SET @n = 99").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE PROCEDURE gap_uv() BEGIN DECLARE n INT DEFAULT 1; SELECT n, @n; END")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("CALL gap_uv()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string(), "99".to_string()]]);
            }
            other => panic!("local n vs @n mismatch, got {other:?}"),
        }

        match client
            .query("CREATE PROCEDURE bad_decl() BEGIN SET n = 1; DECLARE n INT; END")
            .await
        {
            QueryResponse::Err { code: 1337, .. } => {}
            other => panic!("DECLARE after statements must errno 1337, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M166: SIGNAL / RESIGNAL from stored procedures reach the client as ERR packets.
    #[tokio::test]
    async fn procedure_signal_resignal() {
        let server = TestServer::start("procedure_signal").await;
        let mut client = server.connect().await;

        assert!(matches!(
            client
                .query("CREATE PROCEDURE sig_nope() BEGIN SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'nope', MYSQL_ERRNO = 1644; END")
                .await,
            QueryResponse::Ok { .. }
        ));

        match client.query("CALL sig_nope()").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1644);
                assert!(
                    message.contains("nope"),
                    "MESSAGE_TEXT must reach client, got {message}"
                );
            }
            other => panic!("CALL sig_nope must ERR, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE PROCEDURE sig_def() BEGIN SIGNAL SQLSTATE '45000'; END")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("CALL sig_def()").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1644);
                assert!(
                    message.contains("Unhandled user-defined exception condition"),
                    "default SIGNAL message, got {message}"
                );
            }
            other => panic!("CALL sig_def must ERR 1644, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE PROCEDURE sig_re() BEGIN RESIGNAL; END")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("CALL sig_re()").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1645);
                assert!(
                    message.contains("RESIGNAL when handler not active"),
                    "got {message}"
                );
            }
            other => panic!("RESIGNAL outside handler must be 1645, got {other:?}"),
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

    /// M163: `LOCK TABLES … WRITE` blocks other sessions' writers until `UNLOCK TABLES`.
    #[tokio::test]
    async fn lock_tables_write_blocks_insert_until_unlock() {
        let server = TestServer::start("lock_tables_write").await;
        let mut setup = server.connect().await;
        for sql in [
            "CREATE TABLE lt_w (id INT PRIMARY KEY, n INT)",
            "INSERT INTO lt_w VALUES (1, 0)",
        ] {
            assert!(matches!(setup.query(sql).await, QueryResponse::Ok { .. }));
        }
        setup.quit().await;

        let mut a = server.connect().await;
        assert!(matches!(
            a.query("LOCK TABLES lt_w WRITE").await,
            QueryResponse::Ok { .. }
        ));
        assert!(
            server.table_locks.in_lock_tables_mode(a.connection_id),
            "LOCK TABLES WRITE should enter lock mode"
        );
        assert!(
            server
                .table_locks
                .wait_for_access(
                    a.connection_id + 1000,
                    "lt_w",
                    true,
                    std::time::Duration::ZERO,
                )
                .is_err(),
            "foreign writer should see held WRITE lock"
        );

        let addr = server.addr;
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut b = WireClient::connect_addr(addr).await;
            let resp = b.query("INSERT INTO lt_w VALUES (2, 1)").await;
            let _ = tx.send(resp);
            b.quit().await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(
            rx.try_recv().is_err(),
            "INSERT should block while WRITE lock is held"
        );
        assert!(matches!(
            a.query("UNLOCK TABLES").await,
            QueryResponse::Ok { .. }
        ));

        let b_resp = tokio::time::timeout(std::time::Duration::from_secs(3), rx)
            .await
            .expect("waiter timed out")
            .expect("waiter dropped");
        assert!(
            matches!(b_resp, QueryResponse::Ok { .. }),
            "INSERT proceeds after UNLOCK, got {b_resp:?}"
        );
        a.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M163: `LOCK TABLES … READ` allows concurrent SELECT, blocks writers.
    #[tokio::test]
    async fn lock_tables_read_allows_select_blocks_insert() {
        let server = TestServer::start("lock_tables_read").await;
        let mut setup = server.connect().await;
        assert!(matches!(
            setup
                .query("CREATE TABLE lt_r (id INT PRIMARY KEY, n INT)")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            setup.query("INSERT INTO lt_r VALUES (1, 0)").await,
            QueryResponse::Ok { .. }
        ));
        setup.quit().await;

        let mut a = server.connect().await;
        let mut b = server.connect().await;
        assert!(matches!(
            a.query("LOCK TABLES lt_r READ").await,
            QueryResponse::Ok { .. }
        ));
        match b.query("SELECT id FROM lt_r").await {
            QueryResponse::Rows { rows, .. } => assert_eq!(rows, vec![vec!["1".to_string()]]),
            other => panic!("concurrent SELECT under READ lock, got {other:?}"),
        }
        match b.query("SET SESSION innodb_lock_wait_timeout = 1").await {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected SET timeout ok, got {other:?}"),
        }
        match b.query("INSERT INTO lt_r VALUES (2, 1)").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1205),
            other => panic!("expected errno 1205 on blocked INSERT, got {other:?}"),
        }
        assert!(matches!(
            a.query("UNLOCK TABLES").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            b.query("INSERT INTO lt_r VALUES (2, 1)").await,
            QueryResponse::Ok { .. }
        ));
        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M163: unknown table on `LOCK TABLES` returns errno 1146.
    #[tokio::test]
    async fn lock_tables_unknown_table_errno_1146() {
        let server = TestServer::start("lock_tables_1146").await;
        let mut c = server.connect().await;
        match c.query("LOCK TABLES no_such_lt WRITE").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1146),
            other => panic!("expected 1146, got {other:?}"),
        }
        c.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M163: `LOCK TABLES` implicitly commits an open transaction.
    #[tokio::test]
    async fn lock_tables_implicit_commit_open_transaction() {
        let server = TestServer::start("lock_tables_implicit_commit").await;
        let mut setup = server.connect().await;
        for sql in [
            "CREATE TABLE lt_ic (id INT PRIMARY KEY, n INT)",
            "INSERT INTO lt_ic VALUES (1, 0)",
        ] {
            assert!(matches!(setup.query(sql).await, QueryResponse::Ok { .. }));
        }
        setup.quit().await;

        let mut a = server.connect().await;
        assert!(matches!(a.query("BEGIN").await, QueryResponse::Ok { .. }));
        assert!(matches!(
            a.query("INSERT INTO lt_ic VALUES (2, 2)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            a.query("LOCK TABLES lt_ic WRITE").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            a.query("UNLOCK TABLES").await,
            QueryResponse::Ok { .. }
        ));

        let mut b = server.connect().await;
        match b.query("SELECT id, n FROM lt_ic ORDER BY id").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "0".to_string()],
                        vec!["2".to_string(), "2".to_string()]
                    ]
                );
            }
            other => panic!("expected committed row 2, got {other:?}"),
        }
        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M157: `SELECT … FOR UPDATE` waits on same-row writers until commit or lock wait timeout.
    #[tokio::test]
    async fn for_update_wait_update_blocks_until_commit() {
        let server = TestServer::start("for_update_wait").await;
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
        setup.quit().await;

        let mut a = server.connect().await;
        assert!(matches!(a.query("BEGIN").await, QueryResponse::Ok { .. }));
        match a.query("SELECT id FROM fu_t WHERE id = 1 FOR UPDATE").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected FOR UPDATE row, got {other:?}"),
        }
        let lock_key = rusql_storage::RowKey::new("fu_t", vec!["1".into()]);
        assert!(
            server
                .row_locks
                .as_ref()
                .is_held_by(a.connection_id, &lock_key),
            "FOR UPDATE should hold row lock until COMMIT"
        );

        let addr = server.addr;
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut b = WireClient::connect_addr(addr).await;
            assert!(matches!(b.query("BEGIN").await, QueryResponse::Ok { .. }));
            let resp = b
                .query("UPDATE fu_t SET name = 'blocked' WHERE id = 1")
                .await;
            let _ = tx.send(resp);
            b.quit().await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(
            rx.try_recv().is_err(),
            "UPDATE on locked row should block until COMMIT"
        );
        assert!(matches!(a.query("COMMIT").await, QueryResponse::Ok { .. }));

        let b_resp = tokio::time::timeout(std::time::Duration::from_secs(3), rx)
            .await
            .expect("waiter timed out")
            .expect("waiter dropped");
        assert!(
            matches!(b_resp, QueryResponse::Ok { .. }),
            "UPDATE proceeds after holder COMMIT, got {b_resp:?}"
        );

        a.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    #[tokio::test]
    async fn for_update_wait_times_out_with_errno_1205() {
        let server = TestServer::start("for_update_wait_timeout").await;
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
        setup.quit().await;

        let mut a = server.connect().await;
        let mut b = server.connect().await;
        assert!(matches!(a.query("BEGIN").await, QueryResponse::Ok { .. }));
        assert!(matches!(
            b.query("SET SESSION innodb_lock_wait_timeout = 1").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(b.query("BEGIN").await, QueryResponse::Ok { .. }));
        match a.query("SELECT id FROM fu_t WHERE id = 1 FOR UPDATE").await {
            QueryResponse::Rows { .. } => {}
            other => panic!("expected FOR UPDATE row, got {other:?}"),
        }
        match b
            .query("UPDATE fu_t SET name = 'timeout' WHERE id = 1")
            .await
        {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1205, "expected lock wait timeout errno");
                assert!(
                    message.to_ascii_lowercase().contains("lock")
                        || message.contains("锁")
                        || message.contains("timeout"),
                    "expected i18n lock wait message, got {message}"
                );
            }
            other => panic!("expected errno 1205 on lock wait timeout, got {other:?}"),
        }
        assert!(matches!(
            a.query("ROLLBACK").await,
            QueryResponse::Ok { .. }
        ));
        a.quit().await;
        b.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M161: `FOR UPDATE` range scan gap lock blocks phantom INSERT until commit.
    #[tokio::test]
    async fn gap_lock_range_for_update_blocks_phantom_insert() {
        let server = TestServer::start("gap_lock").await;
        let mut setup = server.connect().await;
        for sql in [
            "CREATE TABLE gap_ph (id INT PRIMARY KEY, v VARCHAR(8))",
            "INSERT INTO gap_ph VALUES (10, 'a'), (20, 'b')",
        ] {
            assert!(matches!(setup.query(sql).await, QueryResponse::Ok { .. }));
        }
        setup.quit().await;

        let mut a = server.connect().await;
        assert!(matches!(a.query("BEGIN").await, QueryResponse::Ok { .. }));
        match a
            .query("SELECT id FROM gap_ph WHERE id BETWEEN 10 AND 20 FOR UPDATE")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 2);
            }
            other => panic!("expected FOR UPDATE range rows, got {other:?}"),
        }
        let gap = rusql_storage::GapLockKey::between("gap_ph", "10".into(), "20".into());
        assert!(
            server.row_locks.is_gap_held_by(a.connection_id, &gap),
            "range FOR UPDATE should hold gap lock until COMMIT"
        );

        let addr = server.addr;
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut b = WireClient::connect_addr(addr).await;
            assert!(matches!(b.query("BEGIN").await, QueryResponse::Ok { .. }));
            let resp = b.query("INSERT INTO gap_ph VALUES (15, 'phantom')").await;
            let _ = tx.send(resp);
            b.quit().await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(
            rx.try_recv().is_err(),
            "INSERT into locked gap should block until holder COMMIT"
        );
        assert!(matches!(a.query("COMMIT").await, QueryResponse::Ok { .. }));

        let b_resp = tokio::time::timeout(std::time::Duration::from_secs(3), rx)
            .await
            .expect("insert waiter timed out")
            .expect("insert waiter dropped");
        assert!(
            matches!(b_resp, QueryResponse::Ok { .. }),
            "INSERT proceeds after gap lock release, got {b_resp:?}"
        );

        a.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M160: opposite-order row locks deadlock; younger txn gets errno 1213 (i18n).
    #[tokio::test]
    async fn deadlock_opposite_update_order_errno_1213() {
        let server = TestServer::start("deadlock").await;
        let mut setup = server.connect().await;
        for sql in [
            "CREATE TABLE dl_t (id INT PRIMARY KEY, v VARCHAR(8))",
            "INSERT INTO dl_t VALUES (1, 'a'), (2, 'b')",
        ] {
            assert!(matches!(setup.query(sql).await, QueryResponse::Ok { .. }));
        }
        setup.quit().await;

        let addr = server.addr;
        let (tx_out, rx_out) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut a = WireClient::connect_addr(addr).await;
            let mut b = WireClient::connect_addr(addr).await;
            assert!(matches!(a.query("BEGIN").await, QueryResponse::Ok { .. }));
            assert!(matches!(b.query("BEGIN").await, QueryResponse::Ok { .. }));
            assert!(matches!(
                a.query("UPDATE dl_t SET v = 'a1' WHERE id = 1").await,
                QueryResponse::Ok { .. }
            ));
            assert!(matches!(
                b.query("UPDATE dl_t SET v = 'b2' WHERE id = 2").await,
                QueryResponse::Ok { .. }
            ));

            let (tx_a, rx_a) = tokio::sync::oneshot::channel();
            tokio::spawn(async move {
                let resp = a.query("UPDATE dl_t SET v = 'a2' WHERE id = 2").await;
                let _ = tx_a.send(resp);
                a.quit().await;
            });

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let b_resp = b.query("UPDATE dl_t SET v = 'b1' WHERE id = 1").await;
            let a_resp = tokio::time::timeout(std::time::Duration::from_secs(3), rx_a)
                .await
                .expect("connection A should not hang past lock wait timeout")
                .expect("connection A dropped");
            let _ = tx_out.send((b_resp, a_resp));
            b.quit().await;
        });

        let (b_resp, a_resp) = tokio::time::timeout(std::time::Duration::from_secs(5), rx_out)
            .await
            .expect("deadlock scenario timed out")
            .expect("deadlock task dropped");
        match b_resp {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1213, "younger connection should get deadlock errno");
                assert!(
                    message.to_ascii_lowercase().contains("deadlock"),
                    "expected i18n deadlock message, got {message}"
                );
            }
            other => panic!("expected errno 1213 on deadlock, got {other:?}"),
        }
        assert!(
            matches!(a_resp, QueryResponse::Ok { .. }),
            "survivor UPDATE should succeed after deadlock victim rollback, got {a_resp:?}"
        );

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

    /// M113: SUBSTRING / SUBSTR, ROUND, DATE_ADD in SELECT projections.
    #[tokio::test]
    async fn substring_round_date_add() {
        let server = TestServer::start("substring_round_date_add").await;
        let mut client = server.connect().await;

        match client.query("SELECT SUBSTRING('abc', 1, 2)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["ab".to_string()]]);
            }
            other => panic!("expected SUBSTRING rows, got {other:?}"),
        }
        match client.query("SELECT SUBSTR('abc', 1, 2)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["ab".to_string()]]);
            }
            other => panic!("expected SUBSTR rows, got {other:?}"),
        }
        match client.query("SELECT ROUND(1.4)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected ROUND(1.4) rows, got {other:?}"),
        }
        match client.query("SELECT ROUND(1.5)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["2".to_string()]]);
            }
            other => panic!("expected ROUND(1.5) rows, got {other:?}"),
        }
        match client
            .query("SELECT DATE_ADD('2026-01-01', INTERVAL 1 DAY)")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["2026-01-02".to_string()]]);
            }
            other => panic!("expected DATE_ADD rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M141: TRIM / REPLACE / SUBSTRING_INDEX in SELECT projections.
    #[tokio::test]
    async fn string_pack() {
        let server = TestServer::start("string_pack").await;
        let mut client = server.connect().await;

        match client.query("SELECT TRIM('  a  ')").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["a".to_string()]]);
            }
            other => panic!("expected TRIM rows, got {other:?}"),
        }
        match client.query("SELECT TRIM(LEADING 'x' FROM 'xxa')").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["a".to_string()]]);
            }
            other => panic!("expected TRIM LEADING rows, got {other:?}"),
        }
        match client.query("SELECT REPLACE('abc', 'b', 'x')").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["axc".to_string()]]);
            }
            other => panic!("expected REPLACE rows, got {other:?}"),
        }
        match client
            .query("SELECT SUBSTRING_INDEX('a.b.c', '.', 2)")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["a.b".to_string()]]);
            }
            other => panic!("expected SUBSTRING_INDEX rows, got {other:?}"),
        }
        match client
            .query("SELECT SUBSTRING_INDEX('a.b.c', '.', -2)")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["b.c".to_string()]]);
            }
            other => panic!("expected SUBSTRING_INDEX negative rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M115: JSON_EXTRACT('{"a":1}', '$.a') returns MySQL's unquoted `1`.
    #[tokio::test]
    async fn json_extract() {
        let server = TestServer::start("json_extract").await;
        let mut client = server.connect().await;

        match client
            .query("SELECT JSON_EXTRACT('{\"a\":1}', '$.a')")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected JSON_EXTRACT rows, got {other:?}"),
        }
        match client
            .query("SELECT JSON_EXTRACT('{\"a\":1}', '$.nope')")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["".to_string()]]);
            }
            other => panic!("expected missing-path NULL cell, got {other:?}"),
        }
        match client.query("SELECT JSON_EXTRACT('not json', '$.a')").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 3141);
                assert!(
                    message.to_ascii_lowercase().contains("json"),
                    "expected i18n invalid JSON text, got {message}"
                );
            }
            other => panic!("expected errno 3141, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M133: JSON_UNQUOTE and -> / ->> match JSON_EXTRACT / JSON_UNQUOTE(JSON_EXTRACT).
    #[tokio::test]
    async fn json_unquote() {
        let server = TestServer::start("json_unquote").await;
        let mut client = server.connect().await;

        match client
            .query("SELECT JSON_UNQUOTE(JSON_EXTRACT('{\"a\":\"x\"}', '$.a'))")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["x".to_string()]]);
            }
            other => panic!("expected JSON_UNQUOTE rows, got {other:?}"),
        }
        match client
            .query("SELECT JSON_EXTRACT('{\"a\":\"x\"}', '$.a')")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["\"x\"".to_string()]]);
            }
            other => panic!("expected quoted JSON_EXTRACT string, got {other:?}"),
        }

        assert!(matches!(
            client
                .query("CREATE TABLE ju_t (id INT PRIMARY KEY, j JSON)")
                .await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("INSERT INTO ju_t VALUES (1, '{\"a\":\"x\"}')")
                .await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT j->'$.a' FROM ju_t").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["\"x\"".to_string()]]);
            }
            other => panic!("expected -> extract, got {other:?}"),
        }
        match client.query("SELECT j->>'$.a' FROM ju_t").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["x".to_string()]]);
            }
            other => panic!("expected ->> unquote, got {other:?}"),
        }
        match client
            .query("SELECT JSON_UNQUOTE(JSON_EXTRACT('{\"a\":1}', '$.nope'))")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["".to_string()]]);
            }
            other => panic!("expected missing-path NULL cell, got {other:?}"),
        }
        match client.query("SELECT 'not json'->'$.a'").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 3141);
                assert!(
                    message.to_ascii_lowercase().contains("json"),
                    "expected i18n invalid JSON text, got {message}"
                );
            }
            other => panic!("expected errno 3141, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M140: DATE_SUB, DATEDIFF, DATE_FORMAT portable subset.
    #[tokio::test]
    async fn date_pack() {
        let server = TestServer::start("date_pack").await;
        let mut client = server.connect().await;

        match client
            .query("SELECT DATE_SUB('2026-01-02', INTERVAL 1 DAY)")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["2026-01-01".to_string()]]);
            }
            other => panic!("expected DATE_SUB rows, got {other:?}"),
        }
        match client
            .query("SELECT DATEDIFF('2026-01-02', '2026-01-01')")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected DATEDIFF rows, got {other:?}"),
        }
        match client
            .query("SELECT DATE_FORMAT('2026-01-02', '%Y-%m-%d')")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["2026-01-02".to_string()]]);
            }
            other => panic!("expected DATE_FORMAT rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M139: `CONVERT(… USING utf8mb4)` and charset `CONVERT(…, CHAR CHARACTER SET …)`.
    #[tokio::test]
    async fn convert_charset() {
        let server = TestServer::start("convert_charset").await;
        let mut client = server.connect().await;

        match client.query("SELECT CONVERT('abc' USING utf8mb4)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["abc".to_string()]]);
            }
            other => panic!("expected CONVERT USING rows, got {other:?}"),
        }
        match client
            .query("SELECT CONVERT('abc', CHAR CHARACTER SET utf8mb4)")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["abc".to_string()]]);
            }
            other => panic!("expected CONVERT CHAR CHARACTER SET rows, got {other:?}"),
        }
        match client.query("SELECT CAST('9' AS CHAR)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["9".to_string()]]);
            }
            other => panic!("expected CAST AS CHAR rows, got {other:?}"),
        }
        match client.query("SELECT CONVERT('abc' USING latin1)").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1115);
                assert!(message.contains("latin1"));
            }
            other => panic!("expected errno 1115, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M143: INSERT … SET assignment-form INSERT.
    #[tokio::test]
    async fn insert_set() {
        let server = TestServer::start("insert_set").await;
        let mut client = server.connect().await;

        match client
            .query("CREATE TABLE m143_t (id INT PRIMARY KEY, n INT)")
            .await
        {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected CREATE OK, got {other:?}"),
        }
        match client.query("INSERT INTO m143_t SET id=1, n=2").await {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 1),
            other => panic!("expected INSERT SET OK, got {other:?}"),
        }
        match client.query("SELECT id, n FROM m143_t").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string(), "2".to_string()]]);
            }
            other => panic!("expected rows, got {other:?}"),
        }
        match client.query("INSERT INTO m143_t SET id=1, n=9").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1062),
            other => panic!("expected duplicate PK errno 1062, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M149: column DEFAULT literals and `DEFAULT (expr)` on INSERT omit.
    #[tokio::test]
    async fn default_expr() {
        let server = TestServer::start("default_expr").await;
        let mut client = server.connect().await;

        match client
            .query("CREATE TABLE m149_t (id INT PRIMARY KEY, n INT DEFAULT (1+2), ts DATETIME DEFAULT CURRENT_TIMESTAMP)")
            .await
        {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected CREATE OK, got {other:?}"),
        }
        match client
            .query("INSERT INTO m149_t (id) VALUES (1), (2)")
            .await
        {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 2),
            other => panic!("expected INSERT OK, got {other:?}"),
        }
        match client.query("SELECT id, n FROM m149_t ORDER BY id").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "3".to_string()],
                        vec!["2".to_string(), "3".to_string()],
                    ]
                );
            }
            other => panic!("expected rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M146: VIRTUAL generated columns (`AS (expr)`).
    #[tokio::test]
    async fn generated_column_virtual() {
        let server = TestServer::start("generated_column_virtual").await;
        let mut client = server.connect().await;

        match client
            .query("CREATE TABLE m146_t (id INT PRIMARY KEY, n INT, g INT AS (n + 1) VIRTUAL)")
            .await
        {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected CREATE OK, got {other:?}"),
        }
        match client
            .query("INSERT INTO m146_t (id, n) VALUES (1, 4), (2, 10)")
            .await
        {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 2),
            other => panic!("expected INSERT OK, got {other:?}"),
        }
        match client
            .query("SELECT id, n, g FROM m146_t ORDER BY id")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "4".to_string(), "5".to_string()],
                        vec!["2".to_string(), "10".to_string(), "11".to_string()],
                    ]
                );
            }
            other => panic!("expected rows, got {other:?}"),
        }
        match client
            .query("INSERT INTO m146_t (id, n, g) VALUES (3, 3, 99)")
            .await
        {
            QueryResponse::Err { code, .. } => assert_eq!(code, 3105),
            other => panic!("expected errno 3105 on INSERT into generated column, got {other:?}"),
        }
        match client.query("UPDATE m146_t SET g = 1 WHERE id = 1").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 3105),
            other => panic!("expected errno 3105 on UPDATE generated column, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M150: INVISIBLE columns omitted from `SELECT *`; explicit column still readable.
    #[tokio::test]
    async fn invisible_column() {
        let server = TestServer::start("invisible_column").await;
        let mut client = server.connect().await;

        match client
            .query("CREATE TABLE m150_t (id INT PRIMARY KEY, n INT INVISIBLE)")
            .await
        {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected CREATE OK, got {other:?}"),
        }
        match client
            .query("INSERT INTO m150_t (id, n) VALUES (1, 7), (2, 8)")
            .await
        {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 2),
            other => panic!("expected INSERT OK, got {other:?}"),
        }
        match client.query("SELECT * FROM m150_t ORDER BY id").await {
            QueryResponse::Rows { columns, rows, .. } => {
                assert_eq!(columns, vec!["id".to_string()]);
                assert_eq!(rows, vec![vec!["1".to_string()], vec!["2".to_string()]]);
            }
            other => panic!("expected SELECT * rows, got {other:?}"),
        }
        match client.query("SELECT n FROM m150_t ORDER BY id").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["7".to_string()], vec!["8".to_string()]]);
            }
            other => panic!("expected SELECT n rows, got {other:?}"),
        }
        match client.query("SHOW CREATE TABLE m150_t").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows[0][1].contains("INVISIBLE"));
            }
            other => panic!("expected SHOW CREATE TABLE, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M151: functional index equality lookup and EXPLAIN ref.
    #[tokio::test]
    async fn functional_index() {
        let server = TestServer::start("functional_index").await;
        let mut client = server.connect().await;

        match client
            .query("CREATE TABLE m151_t (id INT PRIMARY KEY, n INT, INDEX ((n+1)))")
            .await
        {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected CREATE OK, got {other:?}"),
        }
        match client
            .query("INSERT INTO m151_t (id, n) VALUES (1, 1), (2, 5)")
            .await
        {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected INSERT OK, got {other:?}"),
        }
        match client
            .query("SELECT id, n FROM m151_t WHERE n + 1 = 2 ORDER BY id")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string(), "1".to_string()]]);
            }
            other => panic!("expected filtered row, got {other:?}"),
        }
        match client
            .query("EXPLAIN SELECT * FROM m151_t WHERE n + 1 = 2")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows[0][3], "ref");
                assert!(!rows[0][4].is_empty());
            }
            other => panic!("expected EXPLAIN rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M155: RENAME TABLE multi-pair atomic swap.
    #[tokio::test]
    async fn rename_table_multi_pair() {
        let server = TestServer::start("rename_table_multi").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE rtm_a (id INT PRIMARY KEY, tag VARCHAR(16))",
            "CREATE TABLE rtm_b (id INT PRIMARY KEY, tag VARCHAR(16))",
            "INSERT INTO rtm_a VALUES (1, 'a')",
            "INSERT INTO rtm_b VALUES (2, 'b')",
            "RENAME TABLE rtm_a TO rtm_a_old, rtm_b TO rtm_a",
        ] {
            match client.query(sql).await {
                QueryResponse::Ok { .. } => {}
                other => panic!("expected OK for {sql}, got {other:?}"),
            }
        }
        match client.query("SELECT tag FROM rtm_a").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["b".to_string()]]);
            }
            other => panic!("expected swapped rtm_a row, got {other:?}"),
        }
        match client.query("SELECT tag FROM rtm_a_old").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["a".to_string()]]);
            }
            other => panic!("expected rtm_a_old row, got {other:?}"),
        }
        match client
            .query("RENAME TABLE missing TO z, rtm_a TO rtm_b")
            .await
        {
            QueryResponse::Err { code: 1146, .. } => {}
            other => panic!("expected 1146 for unknown table, got {other:?}"),
        }
        match client.query("SELECT tag FROM rtm_a").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["b".to_string()]]);
            }
            other => panic!("rename failure must not partially apply, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M154: CREATE TABLE LIKE / AS SELECT.
    #[tokio::test]
    async fn create_table_like_ctas() {
        let server = TestServer::start("create_table_like_ctas").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE ctas_src (id INT PRIMARY KEY, n INT)",
            "INSERT INTO ctas_src VALUES (1, 10), (2, 20)",
            "CREATE TABLE ctas_like LIKE ctas_src",
        ] {
            match client.query(sql).await {
                QueryResponse::Ok { .. } => {}
                other => panic!("expected OK for {sql}, got {other:?}"),
            }
        }
        match client.query("SELECT id FROM ctas_like").await {
            QueryResponse::Rows { rows, .. } => assert!(rows.is_empty()),
            other => panic!("expected empty LIKE table, got {other:?}"),
        }
        match client
            .query("CREATE TABLE ctas_copy AS SELECT id, n FROM ctas_src ORDER BY id")
            .await
        {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected CTAS OK, got {other:?}"),
        }
        match client
            .query("SELECT id, n FROM ctas_copy ORDER BY id")
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
            other => panic!("expected CTAS rows, got {other:?}"),
        }
        match client.query("CREATE TABLE ctas_bad LIKE no_such").await {
            QueryResponse::Err { code: 1146, .. } => {}
            other => panic!("expected errno 1146 for unknown LIKE source, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M152: RANGE table partitioning — CREATE, INSERT route, SELECT/EXPLAIN prune.
    #[tokio::test]
    async fn partition_range() {
        let server = TestServer::start("partition_range").await;
        let mut client = server.connect().await;

        let ddl = "CREATE TABLE m152_t (id INT PRIMARY KEY) PARTITION BY RANGE (id) \
            (PARTITION p0 VALUES LESS THAN (10), PARTITION p1 VALUES LESS THAN MAXVALUE)";
        match client.query(ddl).await {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected CREATE OK, got {other:?}"),
        }
        match client
            .query("INSERT INTO m152_t (id) VALUES (5), (15)")
            .await
        {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 2),
            other => panic!("expected INSERT OK, got {other:?}"),
        }
        match client.query("SELECT id FROM m152_t WHERE id = 5").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["5".to_string()]]);
            }
            other => panic!("expected row in p0, got {other:?}"),
        }
        match client
            .query("EXPLAIN SELECT * FROM m152_t WHERE id = 5")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows[0][6].contains("p0"));
                assert!(!rows[0][6].contains("p1"));
            }
            other => panic!("expected EXPLAIN rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M148: ENUM/SET column types validate members and canonicalize SET lists.
    #[tokio::test]
    async fn enum_set_types() {
        let server = TestServer::start("enum_set_types").await;
        let mut client = server.connect().await;

        match client
            .query("CREATE TABLE m148_t (id INT PRIMARY KEY, e ENUM('a','b'), s SET('a','b','c'))")
            .await
        {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected CREATE OK, got {other:?}"),
        }
        match client
            .query("INSERT INTO m148_t (id, e, s) VALUES (1, 'a', 'b,a')")
            .await
        {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 1),
            other => panic!("expected INSERT OK, got {other:?}"),
        }
        match client
            .query("SELECT id, e, s FROM m148_t ORDER BY id")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(
                    rows[0],
                    vec!["1".to_string(), "a".to_string(), "a,b".to_string()]
                );
            }
            other => panic!("expected SELECT rows, got {other:?}"),
        }
        match client
            .query("INSERT INTO m148_t (id, e, s) VALUES (2, 'c', 'a')")
            .await
        {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1265),
            other => panic!("expected errno 1265 on invalid ENUM, got {other:?}"),
        }
        match client.query("SHOW CREATE TABLE m148_t").await {
            QueryResponse::Rows { rows, .. } => {
                let ddl = rows[0][1].replace(' ', "");
                assert!(ddl.contains("ENUM('a','b')"));
                assert!(ddl.contains("SET('a','b','c')"));
            }
            other => panic!("expected SHOW CREATE rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M147: CHECK constraints reject violating INSERT/UPDATE with errno 3819.
    #[tokio::test]
    async fn check_constraint() {
        let server = TestServer::start("check_constraint").await;
        let mut client = server.connect().await;

        match client
            .query("CREATE TABLE m147_t (id INT PRIMARY KEY, n INT CHECK (n > 0))")
            .await
        {
            QueryResponse::Ok { .. } => {}
            other => panic!("expected CREATE OK, got {other:?}"),
        }
        match client
            .query("INSERT INTO m147_t (id, n) VALUES (1, 4)")
            .await
        {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 1),
            other => panic!("expected INSERT OK, got {other:?}"),
        }
        match client
            .query("INSERT INTO m147_t (id, n) VALUES (2, 0)")
            .await
        {
            QueryResponse::Err { code, .. } => assert_eq!(code, 3819),
            other => panic!("expected errno 3819 on CHECK violation, got {other:?}"),
        }
        match client.query("UPDATE m147_t SET n = 0 WHERE id = 1").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 3819),
            other => panic!("expected errno 3819 on UPDATE CHECK violation, got {other:?}"),
        }
        match client.query("SHOW CREATE TABLE m147_t").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows[0][1].contains("CHECK (n > 0)"));
            }
            other => panic!("expected SHOW CREATE rows, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M144: two-table INNER JOIN UPDATE / DELETE.
    #[tokio::test]
    async fn multi_table_dml() {
        let server = TestServer::start("multi_table_dml").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE m144_u1 (id INT PRIMARY KEY, n INT)",
            "CREATE TABLE m144_u2 (id INT PRIMARY KEY, n INT)",
            "INSERT INTO m144_u1 VALUES (1, 0), (2, 0)",
            "INSERT INTO m144_u2 VALUES (1, 10)",
        ] {
            match client.query(sql).await {
                QueryResponse::Ok { .. } => {}
                other => panic!("expected OK for {sql}, got {other:?}"),
            }
        }
        match client
            .query(
                "UPDATE m144_u1 INNER JOIN m144_u2 ON m144_u1.id = m144_u2.id SET m144_u1.n = m144_u2.n",
            )
            .await
        {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 1),
            other => panic!("expected UPDATE OK, got {other:?}"),
        }
        match client.query("SELECT id, n FROM m144_u1 ORDER BY id").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "10".to_string()],
                        vec!["2".to_string(), "0".to_string()],
                    ]
                );
            }
            other => panic!("expected rows after UPDATE, got {other:?}"),
        }

        for sql in [
            "CREATE TABLE m144_d1 (id INT PRIMARY KEY, n INT)",
            "CREATE TABLE m144_d2 (id INT PRIMARY KEY, n INT)",
            "INSERT INTO m144_d1 VALUES (1, 1), (2, 2)",
            "INSERT INTO m144_d2 VALUES (1, 10)",
        ] {
            match client.query(sql).await {
                QueryResponse::Ok { .. } => {}
                other => panic!("expected OK for {sql}, got {other:?}"),
            }
        }
        match client
            .query("DELETE m144_d1 FROM m144_d1 INNER JOIN m144_d2 ON m144_d1.id = m144_d2.id")
            .await
        {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 1),
            other => panic!("expected DELETE OK, got {other:?}"),
        }
        match client.query("SELECT id FROM m144_d1 ORDER BY id").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["2".to_string()]]);
            }
            other => panic!("expected rows after DELETE, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M142: GREATEST / LEAST vararg comparison builtins.
    #[tokio::test]
    async fn greatest_least() {
        let server = TestServer::start("greatest_least").await;
        let mut client = server.connect().await;

        match client.query("SELECT GREATEST(1, 3, 2)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["3".to_string()]]);
            }
            other => panic!("expected GREATEST rows, got {other:?}"),
        }
        match client.query("SELECT LEAST(1, 3, 2)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected LEAST rows, got {other:?}"),
        }
        match client.query("SELECT GREATEST(1, NULL)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec![String::new()]]);
            }
            other => panic!("expected GREATEST NULL rows, got {other:?}"),
        }
        match client.query("SELECT GREATEST(1)").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1582);
                assert!(
                    message.to_ascii_lowercase().contains("greatest"),
                    "expected i18n GREATEST arity, got {message}"
                );
            }
            other => panic!("expected errno 1582, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M145: EXPLAIN FORMAT=JSON documented stub (connectors probe this form).
    #[tokio::test]
    async fn explain_json() {
        let server = TestServer::start("explain_json").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE m145_t (id INT PRIMARY KEY, k INT)",
            "CREATE INDEX idx_m145_k ON m145_t (k)",
            "INSERT INTO m145_t VALUES (1, 10)",
        ] {
            match client.query(sql).await {
                QueryResponse::Ok { .. } => {}
                other => panic!("expected OK for {sql}, got {other:?}"),
            }
        }
        match client
            .query("EXPLAIN FORMAT=JSON SELECT * FROM m145_t WHERE id = 1")
            .await
        {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["EXPLAIN".to_string()]);
                let doc: serde_json::Value = serde_json::from_str(&rows[0][0]).expect("JSON cell");
                assert!(doc.get("query_block").is_some());
                assert_eq!(doc["query_block"]["table"]["access_type"], "const");
                assert_eq!(doc["query_block"]["table"]["key"], "PRIMARY");
            }
            other => panic!("expected EXPLAIN JSON rows, got {other:?}"),
        }
        match client
            .query("EXPLAIN SELECT * FROM m145_t WHERE id = 1")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows[0][3], "const");
                assert_eq!(rows[0][4], "PRIMARY");
            }
            other => panic!("expected plain EXPLAIN rows, got {other:?}"),
        }
        match client.query("EXPLAIN ANALYZE SELECT 1").await {
            QueryResponse::Err { message, .. } => {
                assert!(message.contains("ANALYZE"));
            }
            other => panic!("expected EXPLAIN ANALYZE error, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M134: JSON_OBJECT / JSON_ARRAY construct MySQL-shaped JSON documents.
    #[tokio::test]
    async fn json_object() {
        let server = TestServer::start("json_object").await;
        let mut client = server.connect().await;

        match client.query("SELECT JSON_OBJECT('a', 1, 'b', 'x')").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec![r#"{"a": 1, "b": "x"}"#.to_string()]]);
            }
            other => panic!("expected JSON_OBJECT rows, got {other:?}"),
        }
        match client.query("SELECT JSON_ARRAY(1, 'x')").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec![r#"[1, "x"]"#.to_string()]]);
            }
            other => panic!("expected JSON_ARRAY rows, got {other:?}"),
        }
        match client.query("SELECT JSON_OBJECT('a')").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1582);
                assert!(
                    message.to_ascii_lowercase().contains("json_object"),
                    "expected i18n JSON_OBJECT arity, got {message}"
                );
            }
            other => panic!("expected errno 1582, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M134: JSON_SET adds missing keys and replaces existing ones.
    #[tokio::test]
    async fn json_set() {
        let server = TestServer::start("json_set").await;
        let mut client = server.connect().await;

        match client.query("SELECT JSON_SET('{\"a\":1}', '$.b', 2)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec![r#"{"a": 1, "b": 2}"#.to_string()]]);
            }
            other => panic!("expected JSON_SET add rows, got {other:?}"),
        }
        match client.query("SELECT JSON_SET('{\"a\":1}', '$.a', 2)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec![r#"{"a": 2}"#.to_string()]]);
            }
            other => panic!("expected JSON_SET replace rows, got {other:?}"),
        }
        match client.query("SELECT JSON_SET('{}', '$.a', 1)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec![r#"{"a": 1}"#.to_string()]]);
            }
            other => panic!("expected JSON_SET add-on-empty rows, got {other:?}"),
        }
        match client.query("SELECT JSON_SET('{}', '$.a.b', 1)").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["{}".to_string()]]);
            }
            other => panic!("expected JSON_SET missing-parent no-op, got {other:?}"),
        }
        match client.query("SELECT JSON_SET('not json', '$.a', 1)").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 3141);
                assert!(
                    message.to_ascii_lowercase().contains("json_set"),
                    "expected i18n json_set invalid JSON text, got {message}"
                );
            }
            other => panic!("expected errno 3141, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M116: UUID() is RFC 4122 v4 in MySQL 8-4-4-4-12 hex form; two calls differ.
    #[tokio::test]
    async fn uuid() {
        let server = TestServer::start("uuid").await;
        let mut client = server.connect().await;

        let first = match client.query("SELECT UUID()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].len(), 1);
                rows[0][0].clone()
            }
            other => panic!("expected UUID rows, got {other:?}"),
        };
        let second = match client.query("SELECT UUID()").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].len(), 1);
                rows[0][0].clone()
            }
            other => panic!("expected second UUID rows, got {other:?}"),
        };
        assert_eq!(first.len(), 36);
        assert_eq!(first.chars().filter(|c| *c == '-').count(), 4);
        assert!(
            first.bytes().enumerate().all(|(i, c)| {
                if matches!(i, 8 | 13 | 18 | 23) {
                    c == b'-'
                } else {
                    c.is_ascii_hexdigit()
                }
            }),
            "expected 8-4-4-4-12 hex UUID, got {first}"
        );
        assert_ne!(first, second, "two UUID() calls on the same connection");

        match client.query("SELECT UUID(1)").await {
            QueryResponse::Err { message, .. } => {
                let lower = message.to_ascii_lowercase();
                assert!(
                    lower.contains("parameter") || message.contains("参数"),
                    "expected i18n arity error, got {message}"
                );
            }
            other => panic!("expected UUID extra-arg error, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

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

    /// M111: REPLACE INTO inserts on a new PK and delete-then-inserts on conflict.
    #[tokio::test]
    async fn replace_into_new_and_existing_pk() {
        let server = TestServer::start("replace_into").await;
        let mut client = server.connect().await;

        assert!(
            matches!(
                client
                    .query("CREATE TABLE rp_t (id INT PRIMARY KEY, v INT)")
                    .await,
                QueryResponse::Ok { .. }
            ),
            "failed CREATE TABLE rp_t"
        );

        match client.query("REPLACE INTO rp_t VALUES (1, 10)").await {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 1),
            other => panic!("expected REPLACE insert OK, got {other:?}"),
        }
        match client.query("SELECT id, v FROM rp_t").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string(), "10".to_string()]]);
            }
            other => panic!("expected inserted REPLACE row, got {other:?}"),
        }

        match client.query("REPLACE INTO rp_t VALUES (1, 20)").await {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 2),
            other => panic!("expected REPLACE conflict OK, got {other:?}"),
        }
        match client.query("SELECT id, v FROM rp_t").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string(), "20".to_string()]]);
            }
            other => panic!("expected replaced row, got {other:?}"),
        }

        match client.query("INSERT INTO rp_t VALUES (1, 30)").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1062),
            other => panic!("expected duplicate key error, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M112: INSERT IGNORE skips PK conflicts and inserts the rest.
    #[tokio::test]
    async fn insert_ignore_skips_existing_pk() {
        let server = TestServer::start("insert_ignore").await;
        let mut client = server.connect().await;

        assert!(
            matches!(
                client
                    .query("CREATE TABLE ig_t (id INT PRIMARY KEY, v INT)")
                    .await,
                QueryResponse::Ok { .. }
            ),
            "failed CREATE TABLE ig_t"
        );

        match client.query("INSERT INTO ig_t VALUES (1, 10)").await {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 1),
            other => panic!("expected INSERT OK, got {other:?}"),
        }

        match client.query("INSERT IGNORE INTO ig_t VALUES (1, 99)").await {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 0),
            other => panic!("expected INSERT IGNORE skip OK, got {other:?}"),
        }
        match client.query("SELECT id, v FROM ig_t").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string(), "10".to_string()]]);
            }
            other => panic!("expected unchanged row, got {other:?}"),
        }

        match client.query("INSERT IGNORE INTO ig_t VALUES (2, 20)").await {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 1),
            other => panic!("expected INSERT IGNORE insert OK, got {other:?}"),
        }

        match client
            .query("INSERT IGNORE INTO ig_t VALUES (1, 99), (3, 30)")
            .await
        {
            QueryResponse::Ok { affected_rows } => assert_eq!(affected_rows, 1),
            other => panic!("expected multi-row INSERT IGNORE OK, got {other:?}"),
        }
        match client.query("SELECT id, v FROM ig_t ORDER BY id").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "10".to_string()],
                        vec!["2".to_string(), "20".to_string()],
                        vec!["3".to_string(), "30".to_string()],
                    ]
                );
            }
            other => panic!("expected skipped conflict plus new rows, got {other:?}"),
        }

        match client.query("INSERT INTO ig_t VALUES (1, 40)").await {
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

    /// M127: WITH RECURSIVE counting UNION ALL; depth cap is errno 3636.
    #[tokio::test]
    async fn with_recursive_select() {
        let server = TestServer::start("with_recursive").await;
        let mut client = server.connect().await;

        match client
            .query(
                "WITH RECURSIVE cte AS (SELECT 1 AS n UNION ALL SELECT n + 1 FROM cte WHERE n < 3) SELECT n FROM cte",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string()],
                        vec!["2".to_string()],
                        vec!["3".to_string()],
                    ]
                );
            }
            other => panic!("expected recursive CTE rows, got {other:?}"),
        }

        match client
            .query(
                "WITH RECURSIVE cte AS (SELECT 1 AS n UNION ALL SELECT n + 1 FROM cte) SELECT n FROM cte",
            )
            .await
        {
            QueryResponse::Err { code, .. } => assert_eq!(code, 3636),
            other => panic!("expected errno 3636 for recursion cap, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M128: INTERSECT distinct of SELECT results; column mismatch is errno 1222.
    #[tokio::test]
    async fn intersect_select() {
        let server = TestServer::start("intersect").await;
        let mut client = server.connect().await;

        match client.query("SELECT 1 AS n INTERSECT SELECT 1 AS n").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected matching INTERSECT row, got {other:?}"),
        }

        match client.query("SELECT 1 AS n INTERSECT SELECT 2 AS n").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows.is_empty());
            }
            other => panic!("expected empty INTERSECT, got {other:?}"),
        }

        match client
            .query("SELECT 1 AS n INTERSECT SELECT 1 AS n, 2 AS m")
            .await
        {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1222),
            other => panic!("expected errno 1222 for INTERSECT column mismatch, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M136: EXCEPT / EXCEPT ALL of SELECT results; column mismatch is errno 1222.
    #[tokio::test]
    async fn except_select() {
        let server = TestServer::start("except").await;
        let mut client = server.connect().await;

        match client.query("SELECT 1 EXCEPT SELECT 1").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows.is_empty());
            }
            other => panic!("expected empty EXCEPT, got {other:?}"),
        }

        match client.query("SELECT 1 EXCEPT SELECT 2").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected EXCEPT row 1, got {other:?}"),
        }

        for sql in [
            "CREATE TABLE e_a (id INT)",
            "CREATE TABLE e_b (id INT)",
            "INSERT INTO e_a VALUES (1), (1)",
            "INSERT INTO e_b VALUES (1)",
        ] {
            assert!(
                matches!(client.query(sql).await, QueryResponse::Ok { .. }),
                "failed: {sql}"
            );
        }

        match client
            .query("SELECT id FROM e_a EXCEPT ALL SELECT id FROM e_b")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected leftover EXCEPT ALL row, got {other:?}"),
        }

        match client
            .query("SELECT 1 AS n EXCEPT SELECT 1 AS n, 2 AS m")
            .await
        {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1222),
            other => panic!("expected errno 1222 for EXCEPT column mismatch, got {other:?}"),
        }

        match client.query("SELECT 1 AS n INTERSECT SELECT 1 AS n").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()]]);
            }
            other => panic!("expected INTERSECT unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M137: FULL OUTER JOIN / FULL JOIN null-pads unmatched rows on both sides.
    #[tokio::test]
    async fn full_outer_join() {
        let server = TestServer::start("full_outer_join").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE fo_a (id INT PRIMARY KEY, name VARCHAR(16))",
            "CREATE TABLE fo_b (id INT PRIMARY KEY, a_id INT, label VARCHAR(16))",
            "INSERT INTO fo_a VALUES (1, 'alice'), (2, 'bob')",
            "INSERT INTO fo_b VALUES (10, 1, 'x'), (20, 3, 'y')",
        ] {
            assert!(
                matches!(client.query(sql).await, QueryResponse::Ok { .. }),
                "failed: {sql}"
            );
        }

        let expected = vec![
            vec!["1".to_string(), "alice".to_string(), "x".to_string()],
            vec!["2".to_string(), "bob".to_string(), String::new()],
            vec![String::new(), String::new(), "y".to_string()],
        ];
        for sql in [
            "SELECT fo_a.id, fo_a.name, fo_b.label FROM fo_a FULL OUTER JOIN fo_b ON fo_a.id = fo_b.a_id",
            "SELECT fo_a.id, fo_a.name, fo_b.label FROM fo_a FULL JOIN fo_b ON fo_a.id = fo_b.a_id",
        ] {
            match client.query(sql).await {
                QueryResponse::Rows { rows, .. } => {
                    assert_eq!(rows, expected, "{sql}");
                }
                other => panic!("expected FULL OUTER JOIN rows for {sql}, got {other:?}"),
            }
        }

        match client
            .query(
                "SELECT fo_a.id, fo_b.label FROM fo_a LEFT JOIN fo_b ON fo_a.id = fo_b.a_id ORDER BY fo_a.id",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "x".to_string()],
                        vec!["2".to_string(), String::new()],
                    ]
                );
            }
            other => panic!("expected LEFT JOIN unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M138: `VALUES ROW(...)` table-value constructor; column mismatch is errno 1136.
    #[tokio::test]
    async fn values_row_constructor() {
        let server = TestServer::start("values_row").await;
        let mut client = server.connect().await;

        match client.query("VALUES ROW(1), ROW(2)").await {
            QueryResponse::Rows { columns, rows } => {
                assert_eq!(columns, vec!["column_0".to_string()]);
                assert_eq!(rows, vec![vec!["1".to_string()], vec!["2".to_string()]]);
            }
            other => panic!("expected VALUES rows, got {other:?}"),
        }

        match client
            .query("SELECT column_0 FROM (VALUES ROW(1), ROW(2)) AS t ORDER BY column_0")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["1".to_string()], vec!["2".to_string()]]);
            }
            other => panic!("expected derived VALUES rows, got {other:?}"),
        }

        match client.query("VALUES ROW(1), ROW(1, 2)").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1136),
            other => panic!("expected errno 1136 for VALUES column mismatch, got {other:?}"),
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

    /// M129: ROWS BETWEEN frames on ranking windows match unframed ranks.
    #[tokio::test]
    async fn window_frame_rows_select() {
        let server = TestServer::start("window_frame").await;
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

        let expected_numbers = vec![
            vec!["1".to_string(), "1".to_string()],
            vec!["2".to_string(), "2".to_string()],
            vec!["3".to_string(), "3".to_string()],
            vec!["4".to_string(), "4".to_string()],
        ];
        for sql in [
            "SELECT id, ROW_NUMBER() OVER (ORDER BY id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS n FROM t ORDER BY id",
            "SELECT id, ROW_NUMBER() OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) AS n FROM t ORDER BY id",
            "SELECT id, ROW_NUMBER() OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND CURRENT ROW) AS n FROM t ORDER BY id",
            "SELECT id, ROW_NUMBER() OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 1 FOLLOWING) AS n FROM t ORDER BY id",
        ] {
            match client.query(sql).await {
                QueryResponse::Rows { rows, .. } => {
                    assert_eq!(rows, expected_numbers, "{sql}");
                }
                other => panic!("expected framed row numbers for {sql}, got {other:?}"),
            }
        }

        match client
            .query(
                "SELECT grp, id, RANK() OVER (PARTITION BY grp ORDER BY score ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS r FROM t ORDER BY grp, id",
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
            other => panic!("expected framed ranks, got {other:?}"),
        }

        match client
            .query("SELECT ROW_NUMBER() OVER (ORDER BY id RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) FROM t")
            .await
        {
            QueryResponse::Err { message, .. } => {
                let lower = message.to_ascii_lowercase();
                assert!(
                    lower.contains("range")
                        || lower.contains("groups")
                        || lower.contains("frame")
                        || message.contains("框架"),
                    "RANGE must error, got {message}"
                );
            }
            other => panic!("expected RANGE error, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M135: LAG / LEAD / SUM() OVER match MySQL on a 3-row fixture.
    #[tokio::test]
    async fn lag_lead_sum_over_select() {
        let server = TestServer::start("lag_lead_sum").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE t (id INT PRIMARY KEY, g VARCHAR(8), n INT)",
            "INSERT INTO t VALUES (1, 'a', 10)",
            "INSERT INTO t VALUES (2, 'a', 20)",
            "INSERT INTO t VALUES (3, 'b', 5)",
        ] {
            assert!(
                matches!(client.query(sql).await, QueryResponse::Ok { .. }),
                "failed: {sql}"
            );
        }

        match client
            .query("SELECT id, LAG(n, 1) OVER (ORDER BY id) AS p FROM t ORDER BY id")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), String::new()],
                        vec!["2".to_string(), "10".to_string()],
                        vec!["3".to_string(), "20".to_string()],
                    ]
                );
            }
            other => panic!("expected LAG rows, got {other:?}"),
        }

        match client
            .query("SELECT id, LEAD(n) OVER (ORDER BY id) AS nxt FROM t ORDER BY id")
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["1".to_string(), "20".to_string()],
                        vec!["2".to_string(), "5".to_string()],
                        vec!["3".to_string(), String::new()],
                    ]
                );
            }
            other => panic!("expected LEAD rows, got {other:?}"),
        }

        match client
            .query(
                "SELECT g, id, SUM(n) OVER (PARTITION BY g ORDER BY id) AS s FROM t ORDER BY g, id",
            )
            .await
        {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![
                        vec!["a".to_string(), "1".to_string(), "10".to_string()],
                        vec!["a".to_string(), "2".to_string(), "30".to_string()],
                        vec!["b".to_string(), "3".to_string(), "5".to_string()],
                    ]
                );
            }
            other => panic!("expected running SUM, got {other:?}"),
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
                    ]
                );
            }
            other => panic!("expected ranking unchanged, got {other:?}"),
        }

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M173: COM_BINLOG_DUMP includes GTID_LOG_EVENT (33) before row events.
    #[tokio::test]
    async fn gtid_dump_includes_gtid_before_rows() {
        let server = TestServer::start("gtid_dump").await;
        let mut client = server.connect().await;

        for sql in [
            "CREATE TABLE gtid_dump_t (id INT)",
            "BEGIN",
            "INSERT INTO gtid_dump_t VALUES (7)",
            "COMMIT",
        ] {
            let resp = client.query(sql).await;
            assert!(
                matches!(resp, QueryResponse::Ok { .. }),
                "failed: {sql} -> {resp:?}"
            );
        }

        let packets = client.binlog_dump(4).await;
        let gtid_pos = packets
            .iter()
            .position(|p| p.get(5) == Some(&EVENT_TYPE_GTID_LOG))
            .expect("GTID_LOG_EVENT in dump");
        let table_map_pos = packets
            .iter()
            .position(|p| p.get(5) == Some(&EVENT_TYPE_TABLE_MAP))
            .expect("TABLE_MAP in dump");
        assert!(
            gtid_pos < table_map_pos,
            "GTID must precede TABLE_MAP (got {gtid_pos} vs {table_map_pos})"
        );

        let mut reconstructed = BINLOG_MAGIC.to_vec();
        for packet in &packets {
            reconstructed.extend_from_slice(&packet[1..]);
        }
        let dump_path = std::env::temp_dir().join(format!("gtid-dump-{}.bin", std::process::id()));
        std::fs::write(&dump_path, &reconstructed).unwrap();
        let applied = apply_binlog_file(&dump_path, |_schema, sql| {
            assert!(sql.contains("INSERT INTO gtid_dump_t"));
            Ok(())
        })
        .unwrap();
        assert_eq!(applied, 1);
        let _ = std::fs::remove_file(&dump_path);

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M176: `@@gtid_executed` and `SHOW MASTER STATUS` reflect persisted executed GTIDs.
    #[tokio::test]
    async fn gtid_executed_after_commit_and_survives_reload() {
        let server = TestServer::start("gtid_executed").await;
        let mut client = server.connect().await;

        match client.query("SELECT @@global.gtid_executed").await {
            QueryResponse::Rows { rows, .. } => assert!(rows[0][0].is_empty()),
            other => panic!("expected empty gtid_executed on fresh server, got {other:?}"),
        }

        for sql in [
            "CREATE TABLE gtid_exec_t (id INT)",
            "BEGIN",
            "INSERT INTO gtid_exec_t VALUES (1)",
            "COMMIT",
        ] {
            let resp = client.query(sql).await;
            assert!(
                matches!(resp, QueryResponse::Ok { .. }),
                "failed: {sql} -> {resp:?}"
            );
        }

        let executed = match client.query("SELECT @@global.gtid_executed").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(!rows[0][0].is_empty());
                rows[0][0].clone()
            }
            other => panic!("expected gtid_executed after COMMIT, got {other:?}"),
        };

        match client.query("SHOW MASTER STATUS").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows[0][4], executed);
            }
            other => panic!("expected SHOW MASTER STATUS rows, got {other:?}"),
        }

        client.quit().await;
        let reloaded = rusql_storage::load_gtid_executed(&server.data_dir);
        assert_eq!(reloaded, executed);
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
                .any(|p| p.get(5) == Some(&EVENT_TYPE_WRITE_ROWS_V2)),
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

    /// M174: idle follow dump receives HEARTBEAT_LOG_EVENT (type 27); heartbeats are not DML.
    #[tokio::test]
    async fn binlog_heartbeat_idle_follow_receives_heartbeat() {
        let server = TestServer::start("binlog_heartbeat_idle").await;
        let mut sql = server.connect().await;
        assert!(
            matches!(
                sql.query("CREATE TABLE hb_t (id INT)").await,
                QueryResponse::Ok { .. }
            ),
            "CREATE TABLE hb_t"
        );

        let mut dump = server.connect().await;
        dump.send_binlog_dump_with_heartbeat_period(4, 0, 1_000_000_000)
            .await;
        let first =
            tokio::time::timeout(std::time::Duration::from_secs(2), dump.read_binlog_event())
                .await
                .expect("timed out waiting for FORMAT_DESCRIPTION");
        assert_eq!(first[5], 15, "first event should be FORMAT_DESCRIPTION");

        let heartbeat =
            tokio::time::timeout(std::time::Duration::from_secs(3), dump.read_binlog_event())
                .await
                .expect("timed out waiting for heartbeat");
        assert!(
            is_heartbeat_dump_packet(&heartbeat),
            "expected heartbeat dump packet, got {heartbeat:?}"
        );
        assert_eq!(heartbeat[5], HEARTBEAT_LOG_EVENT);
        assert_eq!(heartbeat[5], EVENT_TYPE_HEARTBEAT);

        dump.quit().await;
        sql.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M174: row events still stream after heartbeats on follow dump.
    #[tokio::test]
    async fn binlog_heartbeat_follow_still_delivers_row_events() {
        let server = TestServer::start("binlog_heartbeat_rows").await;
        let mut sql = server.connect().await;
        assert!(
            matches!(
                sql.query("CREATE TABLE hb_rows_t (id INT)").await,
                QueryResponse::Ok { .. }
            ),
            "CREATE TABLE hb_rows_t"
        );

        let mut dump = server.connect().await;
        dump.send_binlog_dump_with_heartbeat_period(4, 0, 1_000_000_000)
            .await;
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), dump.read_binlog_event())
            .await
            .expect("FORMAT_DESCRIPTION timeout");

        for text in ["BEGIN", "INSERT INTO hb_rows_t VALUES (9)", "COMMIT"] {
            let resp = sql.query(text).await;
            assert!(
                matches!(resp, QueryResponse::Ok { .. }),
                "failed: {text} -> {resp:?}"
            );
        }

        let mut saw_write_rows = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline && !saw_write_rows {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let packet = tokio::time::timeout(remaining, dump.read_binlog_event())
                .await
                .expect("timed out waiting for row events after heartbeat");
            match packet.get(5) {
                Some(&EVENT_TYPE_WRITE_ROWS_V2) => saw_write_rows = true,
                Some(&EVENT_TYPE_HEARTBEAT)
                | Some(&EVENT_TYPE_TABLE_MAP)
                | Some(&EVENT_TYPE_GTID_LOG) => {}
                other => panic!("unexpected dump event type: {other:?}"),
            }
        }
        assert!(saw_write_rows, "expected WRITE_ROWS after COMMIT");

        dump.quit().await;
        sql.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M174: one-shot (`BINLOG_DUMP_NON_BLOCK`) dump must not emit heartbeats.
    #[tokio::test]
    async fn binlog_heartbeat_not_on_non_block_dump() {
        let server = TestServer::start("binlog_heartbeat_nonblock").await;
        let mut client = server.connect().await;
        assert!(
            matches!(
                client.query("CREATE TABLE hb_nb_t (id INT)").await,
                QueryResponse::Ok { .. }
            ),
            "CREATE TABLE hb_nb_t"
        );

        let packets = client.binlog_dump(4).await;
        assert!(
            packets
                .iter()
                .all(|p| p.get(5) != Some(&EVENT_TYPE_HEARTBEAT)),
            "non-block dump must not include heartbeat events"
        );

        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    fn semisync_err(payload: &[u8]) -> (u16, String) {
        assert_eq!(
            payload.first(),
            Some(&0xff),
            "expected ERR packet, got {payload:?}"
        );
        assert!(payload.len() >= 3, "truncated ERR packet: {payload:?}");
        let code = u16::from_le_bytes([payload[1], payload[2]]);
        let message = String::from_utf8_lossy(&payload[3..]).into_owned();
        (code, message)
    }

    /// M178: `SET @rpl_semi_sync_slave=1` then dump returns errno 1524 immediately.
    #[tokio::test]
    async fn semisync_dump_request_is_errno_1524() {
        let server = TestServer::start("semisync_dump_req").await;
        let mut client = server.connect().await;
        assert!(matches!(
            client.query("SET @rpl_semi_sync_slave = 1").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("CREATE TABLE semisync_req (id INT)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("INSERT INTO semisync_req VALUES (1)").await,
            QueryResponse::Ok { .. }
        ));
        for flags in [0u16, BINLOG_DUMP_NON_BLOCK] {
            client.send_binlog_dump(4, flags).await;
            let payload =
                tokio::time::timeout(std::time::Duration::from_secs(2), client.read_payload())
                    .await
                    .unwrap_or_else(|_| panic!("semi-sync dump flags {flags} hung"));
            let (code, message) = semisync_err(&payload);
            assert_eq!(code, 1524, "{message}");
            assert!(
                message.contains("rpl_semi_sync_master"),
                "errno 1524 must name the plugin, got {message}"
            );
        }
        client.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M178: dump without a semi-sync request keeps `0x00` + event framing.
    #[tokio::test]
    async fn semisync_without_request_dump_unchanged() {
        let server = TestServer::start("semisync_plain_dump").await;
        let mut client = server.connect().await;
        assert!(matches!(
            client.query("CREATE TABLE semisync_plain (id INT)").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("SET @rpl_semi_sync_slave = 0").await,
            QueryResponse::Ok { .. }
        ));
        let packets = client.binlog_dump(4).await;
        assert!(!packets.is_empty(), "expected FORMAT_DESCRIPTION");
        assert_eq!(packets[0][0], 0x00);
        assert_eq!(
            packets[0][5], 15,
            "FORMAT_DESCRIPTION must stay at the async offset"
        );
        client.quit().await;

        let mut again = server.connect().await;
        assert!(matches!(
            again.query("SET @rpl_semi_sync_slave = 1").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            again.reset_connection().await,
            QueryResponse::Ok { .. }
        ));
        let packets = again.binlog_dump(4).await;
        assert_eq!(packets[0][0], 0x00);
        assert_eq!(packets[0][5], 15);
        again.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
    }

    /// M178: a semi-sync ACK on an async dump is errno 1524, not a silent consume or a wait.
    #[tokio::test]
    async fn semisync_ack_packet_is_errno_1524_without_wait() {
        let server = TestServer::start("semisync_ack").await;
        let mut sql = server.connect().await;
        assert!(matches!(
            sql.query("CREATE TABLE semisync_ack_t (id INT)").await,
            QueryResponse::Ok { .. }
        ));

        let mut dump = server.connect().await;
        dump.send_binlog_dump(4, 0).await;
        let first = tokio::time::timeout(std::time::Duration::from_secs(2), dump.read_payload())
            .await
            .expect("timed out waiting for FORMAT_DESCRIPTION");
        assert_eq!(first.first(), Some(&0x00));
        assert_eq!(first.get(5), Some(&15));
        loop {
            match tokio::time::timeout(std::time::Duration::from_millis(200), dump.read_payload())
                .await
            {
                Ok(payload) if payload.first() == Some(&0x00) => {}
                Ok(payload) => panic!("unexpected dump payload before ACK: {payload:?}"),
                Err(_) => break,
            }
        }

        dump.send_payload(&encode_semi_sync_ack(4, "binlog.000001"))
            .await;
        let err = tokio::time::timeout(std::time::Duration::from_secs(2), dump.read_payload())
            .await
            .expect("semi-sync ACK was ignored; the replica would hang");
        let (code, message) = semisync_err(&err);
        assert_eq!(code, 1524, "{message}");
        assert!(message.contains("rpl_semi_sync_master"), "{message}");

        let insert = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            sql.query("INSERT INTO semisync_ack_t VALUES (1)"),
        )
        .await
        .expect("writer hung waiting for a semi-sync ACK");
        assert!(
            matches!(insert, QueryResponse::Ok { .. }),
            "INSERT must commit on async WAL, got {insert:?}"
        );

        dump.quit().await;
        sql.quit().await;
        let _ = std::fs::remove_dir_all(&server.data_dir);
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
                Some(&EVENT_TYPE_WRITE_ROWS_V2) => saw_write_rows = true,
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
                .any(|p| p.get(5) == Some(&EVENT_TYPE_UPDATE_ROWS_V2)),
            "expected UPDATE_ROWS after COMMIT"
        );
        assert!(
            packets
                .iter()
                .any(|p| p.get(5) == Some(&EVENT_TYPE_DELETE_ROWS_V2)),
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

    /// M180: v2 UPDATE of one column does not mention unchanged columns; bad image is errno 1231.
    #[tokio::test]
    async fn row_event_v2_partial_update_preserves_columns() {
        let server = TestServer::start("row_event_v2").await;
        let mut client = server.connect().await;
        match client.query("SELECT @@binlog_row_image").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["FULL".to_string()]]);
            }
            other => panic!("expected default FULL, got {other:?}"),
        }
        match client.query("SET binlog_row_image = 'NOPE'").await {
            QueryResponse::Err { code, message } => {
                assert_eq!(code, 1231, "{message}");
                assert!(message.contains("binlog_row_image"), "{message}");
            }
            other => panic!("expected errno 1231, got {other:?}"),
        }
        assert!(matches!(
            client.query("SET binlog_row_image = 'MINIMAL'").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT @@binlog_row_image").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["MINIMAL".to_string()]]);
            }
            other => panic!("expected MINIMAL, got {other:?}"),
        }

        for sql in [
            "CREATE TABLE img_t (id INT, name VARCHAR(32), note VARCHAR(32))",
            "BEGIN",
            "INSERT INTO img_t VALUES (1, 'alice', 'keep')",
            "COMMIT",
            "BEGIN",
            "UPDATE img_t SET name = 'bob' WHERE id = 1",
            "COMMIT",
        ] {
            let resp = client.query(sql).await;
            assert!(
                matches!(resp, QueryResponse::Ok { .. }),
                "failed: {sql} -> {resp:?}"
            );
        }

        let packets = client.binlog_dump(4).await;
        assert!(packets
            .iter()
            .any(|p| p.get(5) == Some(&EVENT_TYPE_WRITE_ROWS_V2)));
        assert!(packets
            .iter()
            .any(|p| p.get(5) == Some(&EVENT_TYPE_UPDATE_ROWS_V2)));
        let mut reconstructed = BINLOG_MAGIC.to_vec();
        for packet in &packets {
            reconstructed.extend_from_slice(&packet[1..]);
        }
        let queries = extract_query_events(&reconstructed);
        let update = queries
            .iter()
            .find(|q| q.contains("UPDATE"))
            .expect("update sql");
        assert!(update.contains("name = 'bob'"), "{update}");
        assert!(update.contains("WHERE id = 1"), "{update}");
        assert!(!update.contains("note"), "{update}");
        assert!(
            !update.contains("keep"),
            "unchanged column must stay out of the partial image: {update}"
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

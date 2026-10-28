//! rusql MySQL-compatible server binary.

mod audit;
mod connection;
mod error_log;
mod general_log;
mod prepared;
mod replica_io;
mod slow_log;
mod tls;
mod xa;

#[cfg(test)]
mod compat_suite;
#[cfg(test)]
mod mysql_test_subset;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod wire_fixtures;

use anyhow::Context;
use clap::Parser;
use connection::serve_connection;
use rusql_core::{
    Account, ConnectionRegistry, PrivilegeStore, UserLockRegistry, AUTH_PLUGIN_CACHING_SHA2,
};
use rusql_i18n::init;
use rusql_protocol::HandshakeConfig;
use rusql_storage::{OpenTxnRegistry, PersistentEngine, RowLockRegistry, TableLockRegistry};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock as AsyncRwLock;
use tracing::{info, warn};

/// rusql — MySQL-compatible database server
#[derive(Debug, Parser)]
#[command(name = "rusql-server", version, about)]
struct Args {
    /// TCP port to listen on
    #[arg(short, long, default_value_t = 3306)]
    port: u16,

    /// Data directory for WAL persistence
    #[arg(long, default_value = "rusql-data")]
    data_dir: PathBuf,

    /// Locale (en-US or zh-CN)
    #[arg(long, env = "RUSQL_LOCALE", default_value = "en-US")]
    locale: String,

    /// WAL durability: always (safe default), batch, or none (data-loss risk)
    #[arg(long, default_value = "always")]
    wal_sync: String,

    /// Username for password verification (used with --auth-password)
    #[arg(long, default_value = "root")]
    auth_user: String,

    /// Enable password verification for --auth-user (env: RUSQL_AUTH_PASSWORD)
    #[arg(long, env = "RUSQL_AUTH_PASSWORD")]
    auth_password: Option<String>,

    /// PEM certificate chain for client TLS. Requires `--ssl-key`.
    #[arg(long)]
    ssl_cert: Option<PathBuf>,

    /// PEM private key for `--ssl-cert`. Requires `--ssl-cert`.
    #[arg(long)]
    ssl_key: Option<PathBuf>,

    /// Reject plaintext TCP logins with errno 3159.
    #[arg(long, default_value_t = false)]
    require_secure_transport: bool,

    /// JSON-lines file for connect, COM_QUERY, and disconnect events.
    /// Off when omitted. Not the Oracle MySQL Enterprise Audit plugin.
    #[arg(long)]
    audit_log: Option<PathBuf>,

    /// Write the slow query log. Off by default, so no file is created.
    #[arg(long, default_value_t = false)]
    slow_query_log: bool,

    /// Slow query log path. Default is `{data-dir}/rusql-slow.log`.
    /// The file is created only when the log is enabled and a statement is slow.
    #[arg(long)]
    slow_query_log_file: Option<PathBuf>,

    /// Global `long_query_time` in seconds. `0` logs every finished COM_QUERY when the slow log is on.
    #[arg(long, default_value_t = 10.0)]
    long_query_time: f64,

    /// Write the general query log. Off by default, so no file is created.
    #[arg(long, default_value_t = false)]
    general_log: bool,

    /// General query log path. Default is `{data-dir}/rusql-general.log`.
    /// The file is created only when the log is enabled.
    #[arg(long)]
    general_log_file: Option<PathBuf>,

    /// Error log file. Omitted: tracing stays on stderr and no file is created.
    /// `--log_error` is the same flag.
    #[arg(long, alias = "log_error")]
    log_error: Option<PathBuf>,
}

static CONNECTION_ID: AtomicU32 = AtomicU32::new(1);

fn open_audit_log(
    path: Option<&std::path::Path>,
) -> anyhow::Result<Option<std::sync::Arc<audit::AuditLog>>> {
    let Some(path) = path else {
        return Ok(None);
    };
    if path.as_os_str().is_empty() {
        anyhow::bail!("{}", rusql_i18n::messages::server_audit_open_failed(""));
    }
    let shown = path.display().to_string();
    let log = audit::AuditLog::open(path).map_err(|e| {
        anyhow::anyhow!(
            "{}: {e}",
            rusql_i18n::messages::server_audit_open_failed(&shown)
        )
    })?;
    eprintln!("{}", rusql_i18n::messages::server_audit_not_oracle_plugin());
    Ok(Some(log))
}

fn open_slow_query_log(
    data_dir: &std::path::Path,
    enabled: bool,
    file: Option<&std::path::Path>,
    long_query_time: f64,
) -> anyhow::Result<std::sync::Arc<slow_log::SlowQueryLog>> {
    if !long_query_time.is_finite() || long_query_time < 0.0 {
        anyhow::bail!(
            "{}",
            rusql_i18n::messages::sql_wrong_value_for_var(
                "long_query_time",
                &long_query_time.to_string()
            )
        );
    }
    let path = file
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| data_dir.join(rusql_executor::SLOW_QUERY_LOG_FILE_DEFAULT));
    if path.as_os_str().is_empty() {
        anyhow::bail!(
            "{}",
            rusql_i18n::messages::server_slow_query_log_open_failed("")
        );
    }
    let shown = path.display().to_string();
    let formatted = rusql_executor::format_long_query_time(long_query_time);
    slow_log::SlowQueryLog::open(&path, enabled, &formatted).map_err(|e| {
        anyhow::anyhow!(
            "{}: {e}",
            rusql_i18n::messages::server_slow_query_log_open_failed(&shown)
        )
    })
}

fn open_general_log(
    data_dir: &std::path::Path,
    enabled: bool,
    file: Option<&std::path::Path>,
) -> anyhow::Result<std::sync::Arc<general_log::GeneralLog>> {
    let path = file
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| data_dir.join(rusql_executor::GENERAL_LOG_FILE_DEFAULT));
    if path.as_os_str().is_empty() {
        anyhow::bail!(
            "{}",
            rusql_i18n::messages::server_general_log_open_failed("")
        );
    }
    let shown = path.display().to_string();
    general_log::GeneralLog::open(&path, enabled).map_err(|e| {
        anyhow::anyhow!(
            "{}: {e}",
            rusql_i18n::messages::server_general_log_open_failed(&shown)
        )
    })
}

fn open_error_log(
    path: Option<&std::path::Path>,
) -> anyhow::Result<Option<std::sync::Arc<error_log::ErrorLog>>> {
    let Some(path) = path else {
        return Ok(None);
    };
    if path.as_os_str().is_empty() {
        anyhow::bail!("{}", rusql_i18n::messages::server_error_log_open_failed(""));
    }
    let shown = path.display().to_string();
    let log = error_log::ErrorLog::open(path).map_err(|e| {
        anyhow::anyhow!(
            "{}: {e}",
            rusql_i18n::messages::server_error_log_open_failed(&shown)
        )
    })?;
    Ok(Some(log))
}

fn fail_startup(
    log: &Option<std::sync::Arc<error_log::ErrorLog>>,
    err: anyhow::Error,
) -> anyhow::Error {
    if let Some(log) = log {
        log.record_error(
            0,
            0,
            &rusql_i18n::messages::server_startup_failed(&err.to_string()),
        );
    }
    err
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if std::env::var("RUSQL_LOCALE").is_err() {
        rusql_i18n::set_locale(&args.locale);
    }
    init();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let error_log = open_error_log(args.log_error.as_deref())?;

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    info!("{}", rusql_i18n::messages::server_starting(args.port));
    info!(data_dir = %args.data_dir.display(), "storage initialized");

    let wal_sync = rusql_storage::WalSyncPolicy::from_str(&args.wal_sync)
        .map_err(|e| fail_startup(&error_log, anyhow::anyhow!(e)))?;
    info!(wal_sync = ?wal_sync, "WAL sync policy");

    let engine = Arc::new(AsyncRwLock::new(
        PersistentEngine::open_with_policy(&args.data_dir, wal_sync)
            .context("failed to open storage")
            .map_err(|e| fail_startup(&error_log, e))?,
    ));
    let privileges = Arc::new(AsyncRwLock::new(
        PrivilegeStore::load(&args.data_dir)
            .context("failed to load privileges")
            .map_err(|e| fail_startup(&error_log, e))?,
    ));
    let registry = Arc::new(ConnectionRegistry::new());
    let user_locks = Arc::new(UserLockRegistry::new());
    let row_locks = Arc::new(RowLockRegistry::new());
    let table_locks = Arc::new(TableLockRegistry::new());
    let open_txns = Arc::new(OpenTxnRegistry::new());

    let tls = tls::resolve_tls_acceptor(args.ssl_cert.as_deref(), args.ssl_key.as_deref())
        .map_err(|e| fail_startup(&error_log, e))?;
    let handshake_config = HandshakeConfig {
        require_secure_transport: args.require_secure_transport,
        tls_enabled: tls.is_some(),
        ..HandshakeConfig::default()
    };
    if tls.is_some() {
        info!("TLS enabled (--ssl-cert / --ssl-key)");
    }
    if args.require_secure_transport {
        info!("require_secure_transport enabled");
    }
    let audit =
        open_audit_log(args.audit_log.as_deref()).map_err(|e| fail_startup(&error_log, e))?;
    let slow_log = open_slow_query_log(
        &args.data_dir,
        args.slow_query_log,
        args.slow_query_log_file.as_deref(),
        args.long_query_time,
    )
    .map_err(|e| fail_startup(&error_log, e))?;
    let general_log = open_general_log(
        &args.data_dir,
        args.general_log,
        args.general_log_file.as_deref(),
    )
    .map_err(|e| fail_startup(&error_log, e))?;
    if let Some(password) = args.auth_password {
        let mut store = privileges.write().await;
        store.ensure_account(
            &Account::new(&args.auth_user, "%"),
            password,
            AUTH_PLUGIN_CACHING_SHA2,
        );
        store
            .save(&args.data_dir)
            .context("failed to save bootstrap account")
            .map_err(|e| fail_startup(&error_log, e))?;
        info!("password verification enabled (caching_sha2 + native + RSA)");
    }

    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))
        .map_err(|e| fail_startup(&error_log, e))?;
    if let Some(log) = &error_log {
        log.record_startup(&rusql_i18n::messages::server_starting(args.port));
    }

    loop {
        let (stream, peer) = listener.accept().await?;
        let config = handshake_config.clone();
        let connection_tls = tls.clone();
        let connection_id = CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
        let storage = engine.clone();
        let privs = privileges.clone();
        let dir = args.data_dir.clone();
        let client_host = peer.ip().to_string();
        let reg = registry.clone();
        let locks = user_locks.clone();
        let row_locks = row_locks.clone();
        let table_locks = table_locks.clone();
        let connection_audit = audit.clone();
        let connection_slow_log = slow_log.clone();
        let connection_general_log = general_log.clone();
        let connection_open_txns = open_txns.clone();
        let connection_error_log = error_log.clone();
        info!(%peer, connection_id, "client connected");
        tokio::spawn(async move {
            if let Err(e) = serve_connection(
                stream,
                &config,
                connection_id,
                storage,
                privs,
                reg,
                locks,
                row_locks,
                table_locks,
                dir,
                &client_host,
                connection_tls,
                connection_audit,
                connection_slow_log,
                connection_general_log,
                connection_open_txns,
                connection_error_log.clone(),
            )
            .await
            {
                warn!(%peer, connection_id, error = %e, "connection ended with error");
                if let Some(log) = &connection_error_log {
                    let text = e.to_string();
                    if text != rusql_i18n::messages::protocol_access_denied() {
                        log.record_error(connection_id as u64, 0, &text);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod cli_tests {
    use super::Args;
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn audit_log_flag_is_optional() {
        let off = Args::parse_from(["rusql-server"]);
        assert!(off.audit_log.is_none());
        let on = Args::parse_from(["rusql-server", "--audit-log", "audit.jsonl"]);
        assert_eq!(on.audit_log, Some(PathBuf::from("audit.jsonl")));
    }

    #[test]
    fn slow_query_log_flags_are_optional() {
        let off = Args::parse_from(["rusql-server"]);
        assert!(!off.slow_query_log);
        assert!(off.slow_query_log_file.is_none());
        assert_eq!(off.long_query_time, 10.0);
        let on = Args::parse_from([
            "rusql-server",
            "--slow-query-log",
            "--slow-query-log-file",
            "slow.log",
            "--long-query-time",
            "0",
        ]);
        assert!(on.slow_query_log);
        assert_eq!(on.slow_query_log_file, Some(PathBuf::from("slow.log")));
        assert_eq!(on.long_query_time, 0.0);
    }

    #[test]
    fn general_log_flags_are_optional() {
        let off = Args::parse_from(["rusql-server"]);
        assert!(!off.general_log);
        assert!(off.general_log_file.is_none());
        let on = Args::parse_from([
            "rusql-server",
            "--general-log",
            "--general-log-file",
            "general.log",
        ]);
        assert!(on.general_log);
        assert_eq!(on.general_log_file, Some(PathBuf::from("general.log")));
    }

    #[test]
    fn log_error_flag_is_optional() {
        let off = Args::parse_from(["rusql-server"]);
        assert!(off.log_error.is_none());
        let on = Args::parse_from(["rusql-server", "--log-error", "rusql.err"]);
        assert_eq!(on.log_error, Some(PathBuf::from("rusql.err")));
        let alias = Args::parse_from(["rusql-server", "--log_error=server.err"]);
        assert_eq!(alias.log_error, Some(PathBuf::from("server.err")));
        let err = match super::open_error_log(Some(std::path::Path::new(""))) {
            Err(err) => err,
            Ok(_) => panic!("empty --log-error path must fail"),
        };
        let text = err.to_string();
        assert!(
            text.contains("error log") || text.contains("错误日志"),
            "{text}"
        );
    }
}

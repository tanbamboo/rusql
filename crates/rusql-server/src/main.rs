//! rusql MySQL-compatible server binary.

mod connection;
mod prepared;

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
use rusql_core::PrivilegeStore;
use rusql_i18n::init;
use rusql_protocol::{AuthCredentials, HandshakeConfig};
use rusql_storage::PersistentEngine;
use std::net::SocketAddr;
use std::path::PathBuf;
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

    /// Username for password verification (used with --auth-password)
    #[arg(long, default_value = "root")]
    auth_user: String,

    /// Enable password verification for --auth-user (env: RUSQL_AUTH_PASSWORD)
    #[arg(long, env = "RUSQL_AUTH_PASSWORD")]
    auth_password: Option<String>,
}

static CONNECTION_ID: AtomicU32 = AtomicU32::new(1);

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

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    info!("{}", rusql_i18n::messages::server_starting(args.port));
    info!(data_dir = %args.data_dir.display(), "storage initialized");

    let engine = Arc::new(AsyncRwLock::new(
        PersistentEngine::open(&args.data_dir).context("failed to open storage")?,
    ));
    let privileges = Arc::new(AsyncRwLock::new(
        PrivilegeStore::load(&args.data_dir).context("failed to load privileges")?,
    ));

    let mut handshake_config = HandshakeConfig::default();
    if let Some(password) = args.auth_password {
        handshake_config.auth_credentials = Some(AuthCredentials {
            username: args.auth_user,
            password,
        });
        handshake_config.ensure_caching_sha2_rsa();
        info!("password verification enabled (caching_sha2 + native + RSA)");
    }

    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    loop {
        let (mut stream, peer) = listener.accept().await?;
        let config = handshake_config.clone();
        let connection_id = CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
        let storage = engine.clone();
        let privs = privileges.clone();
        let dir = args.data_dir.clone();
        info!(%peer, connection_id, "client connected");
        tokio::spawn(async move {
            if let Err(e) =
                serve_connection(&mut stream, &config, connection_id, storage, privs, dir).await
            {
                warn!(%peer, connection_id, error = %e, "connection ended with error");
            }
        });
    }
}

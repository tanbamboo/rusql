//! rusql MySQL-compatible server binary.

use anyhow::Context;
use clap::Parser;
use rusql_i18n::init;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

/// rusql — MySQL-compatible database server
#[derive(Debug, Parser)]
#[command(name = "rusql-server", version, about)]
struct Args {
    /// TCP port to listen on
    #[arg(short, long, default_value_t = 3306)]
    port: u16,

    /// Locale (en-US or zh-CN)
    #[arg(long, env = "RUSQL_LOCALE", default_value = "en-US")]
    locale: String,
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

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    info!("{}", rusql_i18n::messages::server_starting(args.port));

    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    loop {
        let (stream, peer) = listener.accept().await?;
        info!(%peer, "client connected");
        tokio::spawn(async move {
            let _ = stream;
            // M1: full handshake handler (see issue #2)
        });
    }
}

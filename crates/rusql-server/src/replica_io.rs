//! Replica IO: connect to master, `COM_BINLOG_DUMP`, apply via M57 applier (M179).

use rusql_core::Session;
use rusql_executor::execute;
use rusql_planner::plan;
use rusql_protocol::{
    caching_sha2_fast_scramble, encode_com_binlog_dump, native_password_scramble, read_packet,
    write_packet, HandshakeResponse, InitialHandshake, PacketWriter, AUTH_PLUGIN_CACHING_SHA2,
    AUTH_PLUGIN_NATIVE, BINLOG_DUMP_NON_BLOCK,
};
use rusql_storage::{
    apply_binlog_bytes, PersistentEngine, ReplicaConfig, ReplicaState, BINLOG_MAGIC,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock as AsyncRwLock;
use tracing::{debug, warn};

const CLIENT_PROTOCOL_41: u32 = 0x0000_0200;
const CLIENT_PLUGIN_AUTH: u32 = 0x0008_0000;
const CLIENT_SECURE_CONNECTION: u32 = 0x0000_8000;
const CLIENT_PLUGIN_AUTH_LENENC: u32 = 0x0020_0000;

/// Spawn a background dump task for `START SLAVE` (one-shot `BINLOG_DUMP_NON_BLOCK`).
pub fn spawn_replica_dump(
    replica: Arc<AsyncRwLock<ReplicaState>>,
    engine: Arc<AsyncRwLock<PersistentEngine>>,
    generation: u64,
) {
    tokio::spawn(async move {
        match run_dump_once(replica.clone(), engine, generation).await {
            Ok(()) => {}
            Err(DumpFail::Io(e)) => {
                warn!(error = %e, "replica IO failed");
                let mut state = replica.write().await;
                if state.status.generation == generation {
                    state.report_io_error(e);
                }
            }
            Err(DumpFail::Sql(e)) => {
                warn!(error = %e, "replica SQL apply failed");
                let mut state = replica.write().await;
                if state.status.generation == generation {
                    state.report_sql_error(e);
                }
            }
        }
    });
}

enum DumpFail {
    Io(String),
    Sql(String),
}

async fn run_dump_once(
    replica: Arc<AsyncRwLock<ReplicaState>>,
    engine: Arc<AsyncRwLock<PersistentEngine>>,
    generation: u64,
) -> Result<(), DumpFail> {
    let cfg = {
        let state = replica.read().await;
        if state.status.generation != generation || !state.status.dump_active {
            return Ok(());
        }
        state
            .config
            .clone()
            .ok_or_else(|| DumpFail::Io(rusql_i18n::messages::sql_replica_not_configured()))?
    };

    let packets = fetch_dump_packets(&cfg).await.map_err(DumpFail::Io)?;
    if !still_active(&replica, generation).await {
        return Ok(());
    }

    let mut reconstructed = BINLOG_MAGIC.to_vec();
    for packet in &packets {
        if packet.first() == Some(&0x00) && packet.len() > 1 {
            reconstructed.extend_from_slice(&packet[1..]);
        }
    }

    let apply_result = {
        let mut eng = engine.write().await;
        apply_binlog_bytes(&reconstructed, None, |_schema, sql| {
            apply_sql_on_engine(&mut eng, sql)
        })
    };

    match apply_result {
        Ok(n) => {
            debug!(applied = n, "replica dump applied");
            let mut state = replica.write().await;
            if state.status.generation == generation {
                state.status.last_io_error.clear();
                state.status.last_sql_error.clear();
                state.status.io_running = true;
                state.status.sql_running = true;
            }
            Ok(())
        }
        Err(e) => Err(DumpFail::Sql(e.to_string())),
    }
}

async fn still_active(replica: &Arc<AsyncRwLock<ReplicaState>>, generation: u64) -> bool {
    let state = replica.read().await;
    state.status.generation == generation && state.status.dump_active
}

async fn fetch_dump_packets(cfg: &ReplicaConfig) -> Result<Vec<Vec<u8>>, String> {
    let addr = format!("{}:{}", cfg.master_host, cfg.master_port);
    let stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(&addr))
        .await
        .map_err(|_| {
            rusql_i18n::messages::sql_replica_io_error(&rusql_i18n::messages::sql_replica_bad_host(
                &cfg.master_host,
                &rusql_i18n::messages::sql_replica_connect_timeout(),
            ))
        })?
        .map_err(|e| {
            rusql_i18n::messages::sql_replica_io_error(&rusql_i18n::messages::sql_replica_bad_host(
                &cfg.master_host,
                &e.to_string(),
            ))
        })?;

    let mut stream = stream;
    let mut hdr = [0u8; 4];
    stream
        .read_exact(&mut hdr)
        .await
        .map_err(|e| rusql_i18n::messages::sql_replica_io_error(&e.to_string()))?;
    let len = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], 0]) as usize;
    let mut payload = vec![0u8; len];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(|e| rusql_i18n::messages::sql_replica_io_error(&e.to_string()))?;
    let hs = InitialHandshake::decode_payload(&payload)
        .map_err(|e| rusql_i18n::messages::sql_replica_io_error(&e.to_string()))?;

    let plugin = hs.auth_plugin_name.as_str();
    let auth_response = auth_bytes(&cfg.master_password, &hs.scramble, plugin);

    let caps = CLIENT_PROTOCOL_41
        | CLIENT_PLUGIN_AUTH
        | CLIENT_SECURE_CONNECTION
        | CLIENT_PLUGIN_AUTH_LENENC;
    let response = HandshakeResponse {
        capabilities: caps,
        username: cfg.master_user.clone(),
        auth_response,
        database: None,
        auth_plugin: Some(plugin.to_string()),
        connect_attributes: vec![],
    };
    stream
        .write_all(&PacketWriter::encode(1, &response.encode_payload()))
        .await
        .map_err(|e| rusql_i18n::messages::sql_replica_io_error(&e.to_string()))?;

    complete_auth(&mut stream, cfg).await?;

    let position = cfg.master_log_pos.unwrap_or(4).min(u32::MAX as u64) as u32;
    let dump = encode_com_binlog_dump(position, BINLOG_DUMP_NON_BLOCK, 2);
    write_packet(&mut stream, 0, &dump)
        .await
        .map_err(|e| rusql_i18n::messages::sql_replica_io_error(&e.to_string()))?;

    let mut packets = Vec::new();
    loop {
        let (_seq, pkt) = read_packet(&mut stream)
            .await
            .map_err(|e| rusql_i18n::messages::sql_replica_io_error(&e.to_string()))?;
        if pkt.first() == Some(&0xff) {
            let msg = String::from_utf8_lossy(if pkt.len() > 3 { &pkt[3..] } else { &pkt });
            return Err(rusql_i18n::messages::sql_replica_io_error(
                &rusql_i18n::messages::sql_replica_dump_error(&msg),
            ));
        }
        // Event packets are 0x00 + ≥19-byte header; OK is a short 0x00 payload.
        if pkt.first() == Some(&0x00) && pkt.len() > 19 {
            packets.push(pkt);
            continue;
        }
        break;
    }
    Ok(packets)
}

async fn complete_auth(stream: &mut TcpStream, cfg: &ReplicaConfig) -> Result<(), String> {
    loop {
        let (_seq, pkt) = read_packet(stream)
            .await
            .map_err(|e| rusql_i18n::messages::sql_replica_io_error(&e.to_string()))?;
        match pkt.first() {
            Some(&0x00) => return Ok(()),
            Some(&0xff) => {
                let msg = String::from_utf8_lossy(if pkt.len() > 3 { &pkt[3..] } else { &pkt });
                return Err(rusql_i18n::messages::sql_replica_io_error(
                    &rusql_i18n::messages::sql_replica_bad_host(&cfg.master_host, &msg),
                ));
            }
            Some(&0xfe) => {
                // Auth switch request.
                let mut pos = 1usize;
                let end = pkt[pos..]
                    .iter()
                    .position(|&b| b == 0)
                    .map(|i| pos + i)
                    .unwrap_or(pkt.len());
                let switch_plugin = String::from_utf8_lossy(&pkt[pos..end]).to_string();
                pos = end + 1;
                let mut scramble = [0u8; 20];
                if pos + 20 <= pkt.len() {
                    scramble.copy_from_slice(&pkt[pos..pos + 20]);
                }
                let auth = auth_bytes(&cfg.master_password, &scramble, &switch_plugin);
                write_packet(stream, 3, &auth)
                    .await
                    .map_err(|e| rusql_i18n::messages::sql_replica_io_error(&e.to_string()))?;
            }
            Some(&0x01) if pkt.as_slice() == [0x01, 0x03] => {
                // Fast auth success; OK follows.
            }
            Some(&0x01) if pkt.as_slice() == [0x01, 0x04] => {
                return Err(rusql_i18n::messages::sql_replica_io_error(
                    &rusql_i18n::messages::sql_replica_full_auth(),
                ));
            }
            _ => {
                return Err(rusql_i18n::messages::sql_replica_io_error(
                    &rusql_i18n::messages::sql_replica_unexpected_auth(),
                ));
            }
        }
    }
}

fn auth_bytes(password: &str, scramble: &[u8; 20], plugin: &str) -> Vec<u8> {
    if password.is_empty() {
        return vec![];
    }
    if plugin == AUTH_PLUGIN_CACHING_SHA2 {
        caching_sha2_fast_scramble(password, scramble).to_vec()
    } else if plugin == AUTH_PLUGIN_NATIVE {
        native_password_scramble(password, scramble).to_vec()
    } else {
        caching_sha2_fast_scramble(password, scramble).to_vec()
    }
}

fn apply_sql_on_engine(
    engine: &mut PersistentEngine,
    sql: &str,
) -> Result<(), rusql_storage::StorageError> {
    let mut session = Session::new(0, "repl");
    for meta in engine.table_metas() {
        session.catalog.create_table(meta);
    }
    let stmts = rusql_sql::parse(sql).map_err(|e| {
        rusql_storage::StorageError::Message(rusql_i18n::messages::sql_replica_sql_error(
            &e.to_string(),
        ))
    })?;
    let plans = plan(&session, stmts);
    execute(engine, &mut session, &plans, None).map_err(|e| {
        rusql_storage::StorageError::Message(rusql_i18n::messages::sql_replica_sql_error(
            &e.to_string(),
        ))
    })?;
    Ok(())
}

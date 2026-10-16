//! XA transaction command handling (M162).

use rusql_core::Session;
use rusql_executor::{session_tx_isolation, QueryResult};
use rusql_protocol::{
    err_packet, ok_packet_for_client, text_resultset_for_client, write_packets, ProtocolError,
};
use rusql_sql::XaStatement;
use rusql_storage::{BinlogWriter, PersistentEngine, RowLockRegistry, TransactionState};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{watch, RwLock as AsyncRwLock};

pub const XA_ERRNO: u16 = 1399;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XaPhase {
    Active,
    Idle,
}

#[derive(Debug)]
pub struct XaConnectionState {
    pub xid: String,
    pub phase: XaPhase,
}

pub fn xa_dml_blocked(xa: &Option<XaConnectionState>) -> bool {
    matches!(
        xa,
        Some(XaConnectionState {
            phase: XaPhase::Idle,
            ..
        })
    )
}

fn new_xa_transaction(session: &Session) -> TransactionState {
    TransactionState::with_isolation(session_tx_isolation(session))
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_xa_statement<S>(
    stream: &mut S,
    session: &Session,
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
    binlog: &Arc<AsyncRwLock<BinlogWriter>>,
    binlog_commits: &watch::Sender<u64>,
    data_dir: &Path,
    txn: &mut Option<TransactionState>,
    xa: &mut Option<XaConnectionState>,
    row_locks: &Arc<RowLockRegistry>,
    stmt: XaStatement,
    seq_start: u8,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let result = match stmt {
        XaStatement::Start { xid } => handle_xa_start(session, txn, xa, xid),
        XaStatement::End { xid } => handle_xa_end(txn, xa, &xid),
        XaStatement::Prepare { xid } => {
            handle_xa_prepare(
                session,
                engine,
                binlog,
                binlog_commits,
                data_dir,
                txn,
                xa,
                row_locks,
                &xid,
            )
            .await
        }
        XaStatement::Commit { xid } => {
            handle_xa_commit_prepared(
                session,
                engine,
                binlog,
                binlog_commits,
                data_dir,
                xa,
                row_locks,
                &xid,
            )
            .await
        }
        XaStatement::Rollback { xid } => {
            handle_xa_rollback_branch_or_prepared(
                session,
                engine,
                binlog,
                binlog_commits,
                data_dir,
                txn,
                xa,
                row_locks,
                &xid,
            )
            .await
        }
        XaStatement::Recover => handle_xa_recover(engine).await,
    };

    match result {
        Ok(r) => write_xa_result(stream, session, r, seq_start, client_caps).await,
        Err((errno, msg)) => {
            let err = err_packet(errno, &msg);
            write_packets(stream, seq_start, &[err]).await?;
            Ok(())
        }
    }
}

fn handle_xa_start(
    session: &Session,
    txn: &mut Option<TransactionState>,
    xa: &mut Option<XaConnectionState>,
    xid: String,
) -> Result<QueryResult, (u16, String)> {
    if xa.is_some() || txn.is_some() {
        return Err((XA_ERRNO, rusql_i18n::messages::sql_xa_rmfail_active()));
    }
    *txn = Some(new_xa_transaction(session));
    *xa = Some(XaConnectionState {
        xid,
        phase: XaPhase::Active,
    });
    Ok(QueryResult::Ok { rows_affected: 0 })
}

fn handle_xa_end(
    txn: &mut Option<TransactionState>,
    xa: &mut Option<XaConnectionState>,
    xid: &str,
) -> Result<QueryResult, (u16, String)> {
    let Some(state) = xa.as_mut() else {
        return Err((XA_ERRNO, rusql_i18n::messages::sql_xa_rmfail_idle()));
    };
    if state.phase != XaPhase::Active {
        return Err((XA_ERRNO, rusql_i18n::messages::sql_xa_rmfail_idle()));
    }
    if state.xid != xid {
        return Err((XA_ERRNO, rusql_i18n::messages::sql_xa_not_exist(xid)));
    }
    state.phase = XaPhase::Idle;
    let _ = txn.as_mut();
    Ok(QueryResult::Ok { rows_affected: 0 })
}

#[allow(clippy::too_many_arguments)]
async fn handle_xa_prepare(
    session: &Session,
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
    binlog: &Arc<AsyncRwLock<BinlogWriter>>,
    binlog_commits: &watch::Sender<u64>,
    data_dir: &Path,
    txn: &mut Option<TransactionState>,
    xa: &mut Option<XaConnectionState>,
    row_locks: &Arc<RowLockRegistry>,
    xid: &str,
) -> Result<QueryResult, (u16, String)> {
    let Some(state) = xa.as_ref() else {
        return Err((XA_ERRNO, rusql_i18n::messages::sql_xa_rmfail_idle()));
    };
    if state.phase != XaPhase::Idle || state.xid != xid {
        return Err((XA_ERRNO, rusql_i18n::messages::sql_xa_rmfail_active()));
    }
    let Some(branch) = txn.take() else {
        return Err((XA_ERRNO, rusql_i18n::messages::sql_xa_rmfail_idle()));
    };
    let records: Vec<_> = branch.pending_records().to_vec();
    {
        let mut eng = engine.write().await;
        if let Err(e) = eng.xa_prepare(xid, &records) {
            *txn = Some(branch);
            return Err(map_storage_xa_err(&e.to_string()));
        }
    }
    *xa = None;
    row_locks.release_all(session.id);
    let _ = (data_dir, binlog, binlog_commits);
    Ok(QueryResult::Ok { rows_affected: 0 })
}

#[allow(clippy::too_many_arguments)]
async fn handle_xa_commit_prepared(
    session: &Session,
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
    binlog: &Arc<AsyncRwLock<BinlogWriter>>,
    binlog_commits: &watch::Sender<u64>,
    data_dir: &Path,
    xa: &mut Option<XaConnectionState>,
    row_locks: &Arc<RowLockRegistry>,
    xid: &str,
) -> Result<QueryResult, (u16, String)> {
    if let Some(state) = xa.as_ref() {
        if state.xid == xid {
            return Err((XA_ERRNO, rusql_i18n::messages::sql_xa_rmfail_active()));
        }
    }
    let records = {
        let mut eng = engine.write().await;
        match eng.xa_commit(xid) {
            Ok(r) => r,
            Err(e) => return Err(map_storage_xa_err(&e.to_string())),
        }
    };
    {
        let mut writer = binlog.write().await;
        if let Err(e) = writer.append_commit(data_dir, &session.database, &records) {
            return Err((1105, e.to_string()));
        }
    }
    if !records.is_empty() {
        binlog_commits.send_modify(|n| *n = n.wrapping_add(1));
    }
    row_locks.release_all(session.id);
    Ok(QueryResult::Ok { rows_affected: 0 })
}

#[allow(clippy::too_many_arguments)]
async fn handle_xa_rollback_branch_or_prepared(
    session: &Session,
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
    binlog: &Arc<AsyncRwLock<BinlogWriter>>,
    binlog_commits: &watch::Sender<u64>,
    data_dir: &Path,
    txn: &mut Option<TransactionState>,
    xa: &mut Option<XaConnectionState>,
    row_locks: &Arc<RowLockRegistry>,
    xid: &str,
) -> Result<QueryResult, (u16, String)> {
    if let Some(state) = xa.as_ref() {
        if state.xid == xid {
            if txn.is_none() {
                return Err((XA_ERRNO, rusql_i18n::messages::sql_xa_rmfail_idle()));
            }
            *txn = None;
            *xa = None;
            row_locks.release_all(session.id);
            return Ok(QueryResult::Ok { rows_affected: 0 });
        }
    }
    {
        let mut eng = engine.write().await;
        if eng.xa_is_prepared(xid) {
            if let Err(e) = eng.xa_rollback(xid) {
                return Err(map_storage_xa_err(&e.to_string()));
            }
        } else {
            return Err((XA_ERRNO, rusql_i18n::messages::sql_xa_not_exist(xid)));
        }
    }
    let _ = binlog;
    let _ = binlog_commits;
    let _ = data_dir;
    row_locks.release_all(session.id);
    Ok(QueryResult::Ok { rows_affected: 0 })
}

async fn handle_xa_recover(
    engine: &Arc<AsyncRwLock<PersistentEngine>>,
) -> Result<QueryResult, (u16, String)> {
    let xids = {
        let eng = engine.read().await;
        eng.prepared_xa_xids()
    };
    let rows: Vec<Vec<String>> = xids
        .into_iter()
        .map(|xid| vec!["1".into(), xid, String::new(), String::new()])
        .collect();
    Ok(QueryResult::Rows {
        columns: vec![
            "formatID".into(),
            "gtrid".into(),
            "bqual".into(),
            "data".into(),
        ],
        rows,
    })
}

fn map_storage_xa_err(msg: &str) -> (u16, String) {
    if msg.contains("XAER_DUPID") {
        (XA_ERRNO, rusql_i18n::messages::storage_xa_duplicate_xid(""))
    } else if msg.contains("XAER_NOTA") {
        (XA_ERRNO, msg.to_string())
    } else {
        (1105, msg.to_string())
    }
}

async fn write_xa_result<S>(
    stream: &mut S,
    session: &Session,
    result: QueryResult,
    seq_start: u8,
    client_caps: u32,
) -> Result<(), ProtocolError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match result {
        QueryResult::Ok { rows_affected } => {
            let ok = ok_packet_for_client(rows_affected, session.last_insert_id, client_caps);
            write_packets(stream, seq_start, &[ok]).await?;
        }
        QueryResult::Rows { columns, rows } => {
            let payloads = text_resultset_for_client(&columns, &rows, client_caps);
            write_packets(stream, seq_start, &payloads).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xa_dml_blocked_only_after_end() {
        assert!(!xa_dml_blocked(&None));
        assert!(!xa_dml_blocked(&Some(XaConnectionState {
            xid: "a".into(),
            phase: XaPhase::Active,
        })));
        assert!(xa_dml_blocked(&Some(XaConnectionState {
            xid: "a".into(),
            phase: XaPhase::Idle,
        })));
    }
}

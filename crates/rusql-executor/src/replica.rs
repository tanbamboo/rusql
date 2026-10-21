//! `CHANGE MASTER TO` / `START SLAVE` / `STOP SLAVE` execution (M179).

use crate::{ExecError, QueryResult};
use rusql_sql::{ChangeMasterOptions, ReplicaControlStatement};
use rusql_storage::ReplicaState;

/// Execute a parsed replica-control statement against shared [`ReplicaState`].
pub fn execute_replica_control(
    state: &mut ReplicaState,
    stmt: ReplicaControlStatement,
) -> Result<QueryResult, ExecError> {
    match stmt {
        ReplicaControlStatement::ChangeMaster(opts) => execute_change_master(state, opts),
        ReplicaControlStatement::StartSlave => {
            state.start_slave()?;
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        ReplicaControlStatement::StopSlave => {
            state.stop_slave();
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
    }
}

fn execute_change_master(
    state: &mut ReplicaState,
    opts: ChangeMasterOptions,
) -> Result<QueryResult, ExecError> {
    state.change_master(
        opts.host,
        opts.port,
        opts.user,
        opts.password,
        opts.log_file,
        opts.log_pos,
    )?;
    Ok(QueryResult::Ok { rows_affected: 0 })
}

/// Subset resultset for `SHOW SLAVE STATUS` / `SHOW REPLICA STATUS`.
pub fn show_slave_status(state: &ReplicaState, executed_gtid_set: &str) -> QueryResult {
    let (columns, row) = state.show_status_row(executed_gtid_set);
    QueryResult::Rows {
        columns,
        rows: vec![row],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_sql::try_parse_replica_control;
    use rusql_storage::ReplicaState;
    use std::net::TcpListener;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rusql-exec-replica-{}-{}-{}",
            std::process::id(),
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn change_master_persists_config() {
        let dir = temp_dir("change_master");
        let mut state = ReplicaState::load(&dir).unwrap();
        let stmt = try_parse_replica_control(
            "CHANGE MASTER TO MASTER_HOST='127.0.0.1', MASTER_PORT=3307, MASTER_USER='repl'",
        )
        .unwrap()
        .unwrap();
        execute_replica_control(&mut state, stmt).unwrap();
        let loaded = ReplicaState::load(&dir).unwrap();
        let cfg = loaded.config.expect("config");
        assert_eq!(cfg.master_host, "127.0.0.1");
        assert_eq!(cfg.master_port, 3307);
        assert_eq!(cfg.master_user, "repl");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn change_master_source_synonym() {
        let dir = temp_dir("source_syn");
        let mut state = ReplicaState::load(&dir).unwrap();
        let stmt = try_parse_replica_control(
            "CHANGE REPLICATION SOURCE TO SOURCE_HOST='db', SOURCE_PORT=3306, SOURCE_USER='u'",
        )
        .unwrap()
        .unwrap();
        execute_replica_control(&mut state, stmt).unwrap();
        assert_eq!(state.config.as_ref().unwrap().master_host.as_str(), "db");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn change_master_start_stop_updates_flags() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let dir = temp_dir("start_stop");
        let mut state = ReplicaState::load(&dir).unwrap();
        let change = try_parse_replica_control(&format!(
            "CHANGE MASTER TO MASTER_HOST='127.0.0.1', MASTER_PORT={port}, MASTER_USER='repl'"
        ))
        .unwrap()
        .unwrap();
        execute_replica_control(&mut state, change).unwrap();
        let start = try_parse_replica_control("START SLAVE").unwrap().unwrap();
        execute_replica_control(&mut state, start).unwrap();
        match show_slave_status(&state, "") {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows[0][3], "Yes");
                assert_eq!(rows[0][4], "Yes");
            }
            other => panic!("expected rows, got {other:?}"),
        }
        let stop = try_parse_replica_control("STOP REPLICA").unwrap().unwrap();
        execute_replica_control(&mut state, stop).unwrap();
        match show_slave_status(&state, "") {
            QueryResult::Rows { rows, .. } => {
                assert_eq!(rows[0][3], "No");
                assert_eq!(rows[0][4], "No");
            }
            other => panic!("expected rows, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

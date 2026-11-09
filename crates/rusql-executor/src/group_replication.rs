//! `START GROUP_REPLICATION` / `STOP GROUP_REPLICATION` execution (M199).

use crate::{ExecError, QueryResult};
use rusql_sql::GroupReplicationStatement;
use rusql_storage::{GroupReplicationError, GroupReplicationState};

/// Execute a parsed Group Replication control statement.
pub fn execute_group_replication(
    state: &mut GroupReplicationState,
    stmt: GroupReplicationStatement,
) -> Result<QueryResult, ExecError> {
    match stmt {
        GroupReplicationStatement::Start => {
            state.start().map_err(map_control)?;
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
        GroupReplicationStatement::Stop => {
            state.stop().map_err(map_control)?;
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
    }
}

fn map_control(err: GroupReplicationError) -> ExecError {
    match err {
        GroupReplicationError::Mysql { code, message } => ExecError::Mysql { code, message },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_sql::try_parse_group_replication;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rusql-exec-gr-{}-{}-{}",
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
    fn start_stop_three_member_membership() {
        let dir = temp_dir("members");
        let mut state = GroupReplicationState::load(&dir).unwrap();
        let start = try_parse_group_replication("START GROUP_REPLICATION")
            .unwrap()
            .unwrap();
        execute_group_replication(&mut state, start).unwrap();
        let rows = state.member_rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter().filter(|r| r.member_role == "PRIMARY").count(),
            1
        );
        let stop = try_parse_group_replication("STOP GROUP_REPLICATION")
            .unwrap()
            .unwrap();
        execute_group_replication(&mut state, stop).unwrap();
        assert!(state.member_rows().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_while_running_is_errno_3093() {
        let dir = temp_dir("running");
        let mut state = GroupReplicationState::load(&dir).unwrap();
        execute_group_replication(&mut state, GroupReplicationStatement::Start).unwrap();
        match execute_group_replication(&mut state, GroupReplicationStatement::Start) {
            Err(ExecError::Mysql { code, .. }) => assert_eq!(code, 3093),
            other => panic!("expected errno 3093, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

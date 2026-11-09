//! `START GROUP_REPLICATION` / `STOP GROUP_REPLICATION` (M199).

use crate::SqlError;

/// Group Replication control statement (M199 membership subset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupReplicationStatement {
    Start,
    Stop,
}

/// When `sql` is a Group Replication control statement, parse it; otherwise `None`.
pub fn try_parse_group_replication(
    sql: &str,
) -> Option<Result<GroupReplicationStatement, SqlError>> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return None;
    }
    let upper = trimmed.to_ascii_uppercase();
    if upper == "START GROUP_REPLICATION" {
        return Some(Ok(GroupReplicationStatement::Start));
    }
    if upper == "STOP GROUP_REPLICATION" {
        return Some(Ok(GroupReplicationStatement::Stop));
    }
    if upper.starts_with("START GROUP_REPLICATION ") || upper.starts_with("STOP GROUP_REPLICATION ")
    {
        return Some(Err(SqlError::Parse(rusql_i18n::messages::sql_parse_error(
            &rusql_i18n::messages::sql_group_replication_options(),
        ))));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_start_and_stop() {
        assert_eq!(
            try_parse_group_replication("START GROUP_REPLICATION")
                .unwrap()
                .unwrap(),
            GroupReplicationStatement::Start
        );
        assert_eq!(
            try_parse_group_replication("stop group_replication;")
                .unwrap()
                .unwrap(),
            GroupReplicationStatement::Stop
        );
    }

    #[test]
    fn rejects_unsupported_options() {
        let err = try_parse_group_replication("START GROUP_REPLICATION BOOTSTRAP")
            .unwrap()
            .unwrap_err();
        assert!(err.to_string().contains("not supported") || err.to_string().contains("不支持"));
    }

    #[test]
    fn ignores_unrelated_sql() {
        assert!(try_parse_group_replication("START SLAVE").is_none());
        assert!(try_parse_group_replication("SELECT 1").is_none());
    }
}

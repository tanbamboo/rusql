//! Group Replication membership subset (M199).
//!
//! Config-driven three-member view persisted as `{data_dir}/group_replication.json`.
//! No XCom / GCS. Not full InnoDB Cluster.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::StorageError;

/// `ER_GROUP_REPLICATION_CONFIGURATION` — catalog cannot form a valid membership.
pub const ER_GROUP_REPLICATION_CONFIGURATION: u16 = 3092;
/// `ER_GROUP_REPLICATION_RUNNING` — `START GROUP_REPLICATION` while already running.
pub const ER_GROUP_REPLICATION_RUNNING: u16 = 3093;

const CATALOG_FILE: &str = "group_replication.json";
const MEMBER_VERSION: &str = "8.0.36";
const DEFAULT_GROUP_NAME: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const DEFAULT_LOCAL_ID: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
const DEFAULT_MEMBER_2: &str = "cccccccc-cccc-cccc-cccc-cccccccccccc";
const DEFAULT_MEMBER_3: &str = "dddddddd-dddd-dddd-dddd-dddddddddddd";

/// Client-visible Group Replication control failure with a MySQL errno.
#[derive(Debug, thiserror::Error)]
pub enum GroupReplicationError {
    /// Errno plus i18n message (`HY000`).
    #[error("{message}")]
    Mysql { code: u16, message: String },
}

impl From<StorageError> for GroupReplicationError {
    fn from(err: StorageError) -> Self {
        Self::Mysql {
            code: 1105,
            message: err.to_string(),
        }
    }
}

fn mysql_err(code: u16, message: impl Into<String>) -> GroupReplicationError {
    GroupReplicationError::Mysql {
        code,
        message: message.into(),
    }
}

/// One configured member endpoint (host/port identity).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMemberEndpoint {
    pub member_id: String,
    pub host: String,
    pub port: u16,
}

/// Persisted Group Replication catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupReplicationCatalog {
    pub group_name: String,
    pub local_member_id: String,
    #[serde(default = "default_true")]
    pub single_primary_mode: bool,
    #[serde(default)]
    pub running: bool,
    pub members: Vec<GroupMemberEndpoint>,
}

fn default_true() -> bool {
    true
}

impl Default for GroupReplicationCatalog {
    fn default() -> Self {
        Self {
            group_name: DEFAULT_GROUP_NAME.to_string(),
            local_member_id: DEFAULT_LOCAL_ID.to_string(),
            single_primary_mode: true,
            running: false,
            members: default_three_members(),
        }
    }
}

fn default_three_members() -> Vec<GroupMemberEndpoint> {
    vec![
        GroupMemberEndpoint {
            member_id: DEFAULT_LOCAL_ID.to_string(),
            host: "127.0.0.1".to_string(),
            port: 3306,
        },
        GroupMemberEndpoint {
            member_id: DEFAULT_MEMBER_2.to_string(),
            host: "127.0.0.1".to_string(),
            port: 3307,
        },
        GroupMemberEndpoint {
            member_id: DEFAULT_MEMBER_3.to_string(),
            host: "127.0.0.1".to_string(),
            port: 3308,
        },
    ]
}

/// One `performance_schema.replication_group_members` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicationGroupMember {
    pub member_id: String,
    pub member_host: String,
    pub member_port: u16,
    pub member_state: String,
    pub member_role: String,
    pub member_version: String,
}

/// In-memory + on-disk Group Replication controller (shared across connections).
#[derive(Debug, Clone, Default)]
pub struct GroupReplicationState {
    pub catalog: GroupReplicationCatalog,
    data_dir: Option<PathBuf>,
}

impl GroupReplicationState {
    /// Empty unbound state (no catalog path yet).
    pub fn new() -> Self {
        Self::default()
    }

    /// Load `{data_dir}/group_replication.json` when present; otherwise seed three logical members.
    pub fn load(data_dir: &Path) -> Result<Self, StorageError> {
        let path = data_dir.join(CATALOG_FILE);
        let mut state = Self {
            catalog: GroupReplicationCatalog::default(),
            data_dir: Some(data_dir.to_path_buf()),
        };
        if !path.exists() {
            return Ok(state);
        }
        let raw = fs::read_to_string(&path).map_err(|e| {
            StorageError::Message(format!("failed to read group_replication.json: {e}"))
        })?;
        let mut catalog: GroupReplicationCatalog = serde_json::from_str(&raw)
            .map_err(|e| StorageError::Message(format!("invalid group_replication.json: {e}")))?;
        if catalog.members.len() < 3 {
            // Keep a usable default rather than leaving a broken catalog silent.
            catalog.members = default_three_members();
            if catalog.local_member_id.is_empty() {
                catalog.local_member_id = DEFAULT_LOCAL_ID.to_string();
            }
        }
        state.catalog = catalog;
        Ok(state)
    }

    fn persist(&self) -> Result<(), StorageError> {
        let Some(dir) = &self.data_dir else {
            return Err(StorageError::Message(
                rusql_i18n::messages::sql_group_replication_no_data_dir(),
            ));
        };
        let path = dir.join(CATALOG_FILE);
        let raw = serde_json::to_string_pretty(&self.catalog).map_err(|e| {
            StorageError::Message(format!("failed to encode group_replication.json: {e}"))
        })?;
        fs::write(&path, raw).map_err(|e| {
            StorageError::Message(format!("failed to write group_replication.json: {e}"))
        })?;
        Ok(())
    }

    /// Ensure three member endpoints exist (in-process logical members when unset).
    fn ensure_three_members(&mut self) {
        if self.catalog.members.len() >= 3 {
            return;
        }
        self.catalog.members = default_three_members();
        if self.catalog.local_member_id.is_empty() {
            self.catalog.local_member_id = DEFAULT_LOCAL_ID.to_string();
        }
        if self.catalog.group_name.is_empty() {
            self.catalog.group_name = DEFAULT_GROUP_NAME.to_string();
        }
        self.catalog.single_primary_mode = true;
    }

    /// `START GROUP_REPLICATION`: mark three members ONLINE in single-primary mode.
    pub fn start(&mut self) -> Result<(), GroupReplicationError> {
        if self.catalog.running {
            return Err(mysql_err(
                ER_GROUP_REPLICATION_RUNNING,
                rusql_i18n::messages::sql_group_replication_running(),
            ));
        }
        self.ensure_three_members();
        if self.catalog.members.len() < 3 {
            return Err(mysql_err(
                ER_GROUP_REPLICATION_CONFIGURATION,
                rusql_i18n::messages::sql_group_replication_configuration(),
            ));
        }
        // Keep exactly three for this membership subset.
        self.catalog.members.truncate(3);
        if !self
            .catalog
            .members
            .iter()
            .any(|m| m.member_id == self.catalog.local_member_id)
        {
            self.catalog.local_member_id = self.catalog.members[0].member_id.clone();
        }
        self.catalog.single_primary_mode = true;
        self.catalog.running = true;
        self.persist()?;
        Ok(())
    }

    /// `STOP GROUP_REPLICATION`: clear running membership.
    pub fn stop(&mut self) -> Result<(), GroupReplicationError> {
        self.catalog.running = false;
        self.persist()?;
        Ok(())
    }

    /// Rows for `performance_schema.replication_group_members` (empty when stopped).
    pub fn member_rows(&self) -> Vec<ReplicationGroupMember> {
        if !self.catalog.running {
            return Vec::new();
        }
        self.catalog
            .members
            .iter()
            .take(3)
            .map(|m| {
                let role = if self.catalog.single_primary_mode {
                    if m.member_id == self.catalog.local_member_id {
                        "PRIMARY"
                    } else {
                        "SECONDARY"
                    }
                } else {
                    "PRIMARY"
                };
                ReplicationGroupMember {
                    member_id: m.member_id.clone(),
                    member_host: m.host.clone(),
                    member_port: m.port,
                    member_state: "ONLINE".to_string(),
                    member_role: role.to_string(),
                    member_version: MEMBER_VERSION.to_string(),
                }
            })
            .collect()
    }
}

/// Load membership rows from `{data_dir}/group_replication.json` when present.
pub fn load_replication_group_members(data_dir: &Path) -> Vec<ReplicationGroupMember> {
    match GroupReplicationState::load(data_dir) {
        Ok(state) => state.member_rows(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rusql-gr-{}-{}-{}",
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
    fn group_replication_start_yields_three_online_members() {
        let dir = temp_dir("start3");
        let mut state = GroupReplicationState::load(&dir).unwrap();
        assert!(state.member_rows().is_empty());
        state.start().unwrap();
        let rows = state.member_rows();
        assert_eq!(rows.len(), 3, "membership must be three nodes");
        assert!(rows.iter().all(|r| r.member_state == "ONLINE"));
        let primaries: Vec<_> = rows.iter().filter(|r| r.member_role == "PRIMARY").collect();
        let secondaries: Vec<_> = rows
            .iter()
            .filter(|r| r.member_role == "SECONDARY")
            .collect();
        assert_eq!(primaries.len(), 1);
        assert_eq!(secondaries.len(), 2);
        assert_eq!(primaries[0].member_id, DEFAULT_LOCAL_ID);
        let loaded = GroupReplicationState::load(&dir).unwrap();
        assert!(loaded.catalog.running);
        assert_eq!(loaded.member_rows().len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn group_replication_stop_clears_members() {
        let dir = temp_dir("stop");
        let mut state = GroupReplicationState::load(&dir).unwrap();
        state.start().unwrap();
        state.stop().unwrap();
        assert!(state.member_rows().is_empty());
        let loaded = GroupReplicationState::load(&dir).unwrap();
        assert!(!loaded.catalog.running);
        assert!(loaded.member_rows().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn group_replication_start_twice_is_errno_3093() {
        let dir = temp_dir("twice");
        let mut state = GroupReplicationState::load(&dir).unwrap();
        state.start().unwrap();
        match state.start() {
            Err(GroupReplicationError::Mysql { code, .. }) => assert_eq!(code, 3093),
            other => panic!("expected errno 3093, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

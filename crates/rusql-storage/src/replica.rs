//! Binlog apply helper and persisted replica role for manual promote (M177).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{apply_binlog_bytes, StorageError};

const ROLE_FILE: &str = "replica-role.json";

pub fn apply_binlog_file<F>(path: &Path, apply_sql: F) -> Result<u64, StorageError>
where
    F: FnMut(&str, &str) -> Result<(), StorageError>,
{
    let bytes =
        std::fs::read(path).map_err(|e| StorageError::Message(format!("read binlog: {e}")))?;
    apply_binlog_bytes(&bytes, None, apply_sql)
}

/// Persisted primary/replica writability (`read_only` / `super_read_only`).
///
/// Missing file means a writable primary. Loading never clears flags (no auto-promote).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaRole {
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub super_read_only: bool,
}

impl ReplicaRole {
    /// Load `{data_dir}/replica-role.json`, or the writable default when absent.
    pub fn load(data_dir: &Path) -> Result<Self, StorageError> {
        let path = data_dir.join(ROLE_FILE);
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(&path).map_err(|_| {
            StorageError::Message(rusql_i18n::messages::sql_replica_role_persist_failed())
        })?;
        if bytes.is_empty() {
            return Ok(Self::default());
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            StorageError::Message(rusql_i18n::messages::sql_replica_role_persist_failed())
        })
    }

    /// Write this role to `{data_dir}/replica-role.json`.
    pub fn save(&self, data_dir: &Path) -> Result<(), StorageError> {
        std::fs::create_dir_all(data_dir).map_err(|_| {
            StorageError::Message(rusql_i18n::messages::sql_replica_role_persist_failed())
        })?;
        let json = serde_json::to_vec_pretty(self).map_err(|_| {
            StorageError::Message(rusql_i18n::messages::sql_replica_role_persist_failed())
        })?;
        std::fs::write(data_dir.join(ROLE_FILE), json).map_err(|_| {
            StorageError::Message(rusql_i18n::messages::sql_replica_role_persist_failed())
        })
    }

    /// MySQL option name when this role blocks `user`, if any.
    ///
    /// `super_read_only` blocks every account, including SUPER. `read_only` blocks
    /// accounts that are not SUPER.
    pub fn blocking_option(&self, is_superuser: bool) -> Option<&'static str> {
        if self.super_read_only {
            return Some("--super-read-only");
        }
        if self.read_only && !is_superuser {
            return Some("--read-only");
        }
        None
    }

    pub fn writes_allowed(&self, is_superuser: bool) -> bool {
        self.blocking_option(is_superuser).is_none()
    }

    /// `SET GLOBAL read_only`. OFF also clears `super_read_only` (MySQL promote).
    pub fn set_read_only(&mut self, on: bool) {
        if on {
            self.read_only = true;
        } else {
            self.promote();
        }
    }

    /// `SET GLOBAL super_read_only`. ON also sets `read_only`. OFF leaves `read_only`.
    pub fn set_super_read_only(&mut self, on: bool) {
        self.super_read_only = on;
        if on {
            self.read_only = true;
        }
    }

    /// Make this data directory accept writes. Does not touch any other directory.
    pub fn promote(&mut self) {
        self.read_only = false;
        self.super_read_only = false;
    }
}

#[cfg(test)]
mod tests {
    use crate::PersistentEngine;

    fn scratch(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rusql-replica-promote-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn replica_promote_persists_and_does_not_touch_other_directory() {
        let primary = scratch("old-primary");
        let replica = scratch("replica");

        let mut old = PersistentEngine::open(&primary).unwrap();
        old.set_replica_super_read_only(true).unwrap();
        assert!(!old.replica_role().writes_allowed(true));
        assert_eq!(
            old.replica_role().blocking_option(true),
            Some("--super-read-only")
        );

        let mut promoted = PersistentEngine::open(&replica).unwrap();
        promoted.set_replica_super_read_only(true).unwrap();
        assert!(!promoted.replica_role().writes_allowed(false));
        promoted.promote_replica().unwrap();
        assert!(promoted.replica_role().writes_allowed(false));
        assert!(promoted.replica_role().blocking_option(true).is_none());

        let promoted_again = PersistentEngine::open(&replica).unwrap();
        assert!(
            promoted_again.replica_role().writes_allowed(false),
            "restart must not clear a promoted role"
        );

        let old_again = PersistentEngine::open(&primary).unwrap();
        assert!(
            !old_again.replica_role().writes_allowed(true),
            "promoting one data directory must not fence or unfence another"
        );
        assert!(old_again.replica_role().read_only);
        assert!(old_again.replica_role().super_read_only);

        let mut fenced = PersistentEngine::open(&primary).unwrap();
        fenced.set_replica_super_read_only(false).unwrap();
        assert!(fenced.replica_role().read_only);
        assert!(!fenced.replica_role().super_read_only);
        assert!(fenced.replica_role().writes_allowed(true));
        assert!(!fenced.replica_role().writes_allowed(false));
        assert_eq!(
            fenced.replica_role().blocking_option(false),
            Some("--read-only")
        );

        let _ = std::fs::remove_dir_all(&primary);
        let _ = std::fs::remove_dir_all(&replica);
    }
}

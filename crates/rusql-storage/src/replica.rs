//! Replica connection config, start/stop, binlog apply (M57 + M179), and manual promote role (M177).

use std::fs;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{apply_binlog_bytes, StorageError};

/// Default MySQL port when `MASTER_PORT` / `SOURCE_PORT` is omitted.
pub const DEFAULT_MASTER_PORT: u16 = 3306;

/// Connect timeout used when probing `MASTER_HOST` on `START SLAVE`.
pub const MASTER_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// `ER_SLAVE_MUST_STOP` — `CHANGE MASTER` while IO or SQL is running.
pub const ER_SLAVE_MUST_STOP: u16 = 1198;
/// `ER_BAD_SLAVE` — `START SLAVE` before `CHANGE MASTER TO`.
pub const ER_BAD_SLAVE: u16 = 1200;
/// `ER_WRONG_VALUE_FOR_VAR` — `MASTER_PORT` / `SOURCE_PORT` of 0.
pub const ER_WRONG_VALUE_FOR_VAR: u16 = 1231;
/// `CR_CONN_HOST_ERROR` — IO thread cannot connect to the source.
pub const CR_CONN_HOST_ERROR: u16 = 2003;

const REPLICA_CATALOG_FILE: &str = "replica.json";
const ROLE_FILE: &str = "replica-role.json";

/// Client-visible replica control failure with a MySQL errno.
#[derive(Debug, thiserror::Error)]
pub enum ReplicaControlError {
    /// Errno plus i18n message (`HY000`).
    #[error("{message}")]
    Mysql { code: u16, message: String },
}

impl From<StorageError> for ReplicaControlError {
    fn from(err: StorageError) -> Self {
        Self::Mysql {
            code: 1105,
            message: err.to_string(),
        }
    }
}

fn mysql_err(code: u16, message: impl Into<String>) -> ReplicaControlError {
    ReplicaControlError::Mysql {
        code,
        message: message.into(),
    }
}

/// Persisted replica connection settings (`CHANGE MASTER TO` / `CHANGE REPLICATION SOURCE TO`).
///
/// Passwords are stored in `{data_dir}/replica.json`. Restrict directory permissions so the
/// catalog is not world-readable; passwords are never written to logs by this module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaConfig {
    pub master_host: String,
    pub master_port: u16,
    pub master_user: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub master_password: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master_log_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub master_log_pos: Option<u64>,
}

impl Default for ReplicaConfig {
    fn default() -> Self {
        Self {
            master_host: String::new(),
            master_port: DEFAULT_MASTER_PORT,
            master_user: String::new(),
            master_password: String::new(),
            master_log_file: None,
            master_log_pos: None,
        }
    }
}

/// Runtime replica status visible via `SHOW SLAVE STATUS` / `SHOW REPLICA STATUS`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReplicaRuntimeStatus {
    pub io_running: bool,
    pub sql_running: bool,
    pub last_io_error: String,
    pub last_sql_error: String,
    /// True while a dump session should be active (server IO task).
    pub dump_active: bool,
    /// Bumped on start/stop so IO tasks can detect cancellation.
    pub generation: u64,
}

/// In-memory + on-disk replica controller (shared across connections).
#[derive(Debug, Clone, Default)]
pub struct ReplicaState {
    pub config: Option<ReplicaConfig>,
    pub status: ReplicaRuntimeStatus,
    data_dir: Option<PathBuf>,
}

impl ReplicaState {
    /// Empty state (no catalog path yet).
    pub fn new() -> Self {
        Self::default()
    }

    /// Load `{data_dir}/replica.json` when present.
    pub fn load(data_dir: &Path) -> Result<Self, StorageError> {
        let path = data_dir.join(REPLICA_CATALOG_FILE);
        let mut state = Self {
            config: None,
            status: ReplicaRuntimeStatus::default(),
            data_dir: Some(data_dir.to_path_buf()),
        };
        if !path.exists() {
            return Ok(state);
        }
        let bytes = fs::read(&path)
            .map_err(|e| StorageError::Message(format!("read {REPLICA_CATALOG_FILE}: {e}")))?;
        if bytes.is_empty() {
            return Ok(state);
        }
        let cfg: ReplicaConfig = serde_json::from_slice(&bytes)
            .map_err(|e| StorageError::Message(format!("parse {REPLICA_CATALOG_FILE}: {e}")))?;
        state.config = Some(cfg);
        Ok(state)
    }

    /// Persist current config to `{data_dir}/replica.json`.
    pub fn save(&self) -> Result<(), StorageError> {
        let Some(dir) = self.data_dir.as_ref() else {
            return Err(StorageError::Message(
                rusql_i18n::messages::sql_replica_no_data_dir().to_string(),
            ));
        };
        let Some(cfg) = self.config.as_ref() else {
            let path = dir.join(REPLICA_CATALOG_FILE);
            if path.exists() {
                let _ = fs::remove_file(&path);
            }
            return Ok(());
        };
        let json = serde_json::to_string_pretty(cfg)
            .map_err(|e| StorageError::Message(format!("serialize replica config: {e}")))?;
        fs::write(dir.join(REPLICA_CATALOG_FILE), json)
            .map_err(|e| StorageError::Message(format!("write {REPLICA_CATALOG_FILE}: {e}")))?;
        Ok(())
    }

    /// Bind this state to a data directory (for tests that build config in memory).
    pub fn set_data_dir(&mut self, data_dir: impl Into<PathBuf>) {
        self.data_dir = Some(data_dir.into());
    }

    /// Apply `CHANGE MASTER TO` / `CHANGE REPLICATION SOURCE TO` options and persist.
    pub fn change_master(
        &mut self,
        host: Option<String>,
        port: Option<u16>,
        user: Option<String>,
        password: Option<String>,
        log_file: Option<String>,
        log_pos: Option<u64>,
    ) -> Result<(), ReplicaControlError> {
        if self.status.io_running || self.status.sql_running || self.status.dump_active {
            return Err(mysql_err(
                ER_SLAVE_MUST_STOP,
                rusql_i18n::messages::sql_replica_running_change_master(),
            ));
        }
        let mut cfg = self.config.clone().unwrap_or_default();
        if let Some(h) = host {
            if h.is_empty() {
                return Err(mysql_err(
                    ER_BAD_SLAVE,
                    rusql_i18n::messages::sql_replica_empty_host(),
                ));
            }
            cfg.master_host = h;
        }
        if let Some(p) = port {
            if p == 0 {
                return Err(mysql_err(
                    ER_WRONG_VALUE_FOR_VAR,
                    rusql_i18n::messages::sql_replica_invalid_port(p),
                ));
            }
            cfg.master_port = p;
        }
        if let Some(u) = user {
            cfg.master_user = u;
        }
        if let Some(pw) = password {
            cfg.master_password = pw;
        }
        if let Some(f) = log_file {
            cfg.master_log_file = if f.is_empty() { None } else { Some(f) };
        }
        if let Some(pos) = log_pos {
            cfg.master_log_pos = Some(pos);
        }
        if cfg.master_host.is_empty() {
            return Err(mysql_err(
                ER_BAD_SLAVE,
                rusql_i18n::messages::sql_replica_empty_host(),
            ));
        }
        self.config = Some(cfg);
        self.save()?;
        Ok(())
    }

    /// `START SLAVE` / `START REPLICA`: probe master TCP, then mark IO/SQL running.
    ///
    /// Bad host/port yields errno [`CR_CONN_HOST_ERROR`] (no panic) and clears running flags.
    pub fn start_slave(&mut self) -> Result<(), ReplicaControlError> {
        let Some(cfg) = self.config.clone() else {
            return Err(mysql_err(
                ER_BAD_SLAVE,
                rusql_i18n::messages::sql_replica_not_configured(),
            ));
        };
        if self.status.dump_active {
            return Ok(());
        }
        self.status.last_io_error.clear();
        self.status.last_sql_error.clear();
        match probe_master_tcp(&cfg.master_host, cfg.master_port) {
            Ok(()) => {
                self.status.io_running = true;
                self.status.sql_running = true;
                self.status.dump_active = true;
                self.status.generation = self.status.generation.wrapping_add(1);
                Ok(())
            }
            Err(msg) => {
                self.status.io_running = false;
                self.status.sql_running = false;
                self.status.dump_active = false;
                self.status.last_io_error = msg.clone();
                Err(mysql_err(CR_CONN_HOST_ERROR, msg))
            }
        }
    }

    /// `STOP SLAVE` / `STOP REPLICA`.
    pub fn stop_slave(&mut self) {
        self.status.io_running = false;
        self.status.sql_running = false;
        self.status.dump_active = false;
        self.status.generation = self.status.generation.wrapping_add(1);
    }

    /// IO thread failed after `START SLAVE`. SQL thread stays up until `STOP SLAVE`.
    pub fn report_io_error(&mut self, message: impl Into<String>) {
        self.status.io_running = false;
        self.status.dump_active = false;
        self.status.last_io_error = message.into();
    }

    /// SQL apply failed. IO thread status is left as the dump left it.
    pub fn report_sql_error(&mut self, message: impl Into<String>) {
        self.status.sql_running = false;
        self.status.last_sql_error = message.into();
    }

    /// Subset columns for `SHOW SLAVE STATUS` / `SHOW REPLICA STATUS`.
    pub fn show_status_row(&self, executed_gtid_set: &str) -> (Vec<String>, Vec<String>) {
        let columns = vec![
            "Master_Host".into(),
            "Master_User".into(),
            "Master_Port".into(),
            "Slave_IO_Running".into(),
            "Slave_SQL_Running".into(),
            "Last_IO_Error".into(),
            "Last_SQL_Error".into(),
            "Retrieved_Gtid_Set".into(),
            "Executed_Gtid_Set".into(),
        ];
        let (host, user, port) = match self.config.as_ref() {
            Some(c) => (
                c.master_host.clone(),
                c.master_user.clone(),
                c.master_port.to_string(),
            ),
            None => (String::new(), String::new(), String::new()),
        };
        let row = vec![
            host,
            user,
            port,
            if self.status.io_running {
                "Yes".into()
            } else {
                "No".into()
            },
            if self.status.sql_running {
                "Yes".into()
            } else {
                "No".into()
            },
            self.status.last_io_error.clone(),
            self.status.last_sql_error.clone(),
            String::new(),
            executed_gtid_set.to_string(),
        ];
        (columns, row)
    }
}

/// Apply binlog QUERY / row events from a file (M57 applier).
pub fn apply_binlog_file<F>(path: &Path, apply_sql: F) -> Result<u64, StorageError>
where
    F: FnMut(&str, &str) -> Result<(), StorageError>,
{
    let bytes =
        std::fs::read(path).map_err(|e| StorageError::Message(format!("read binlog: {e}")))?;
    apply_binlog_bytes(&bytes, None, apply_sql)
}

/// Probe TCP reachability of `host:port` without panicking on DNS/connect failure.
pub fn probe_master_tcp(host: &str, port: u16) -> Result<(), String> {
    let addr = format!("{host}:{port}");
    let addrs: Vec<SocketAddr> = match addr.to_socket_addrs() {
        Ok(iter) => iter.collect(),
        Err(e) => {
            return Err(rusql_i18n::messages::sql_replica_bad_host(
                host,
                &e.to_string(),
            ));
        }
    };
    if addrs.is_empty() {
        return Err(rusql_i18n::messages::sql_replica_bad_host(
            host,
            &rusql_i18n::messages::sql_replica_no_addresses(),
        ));
    }
    let mut last_err = String::new();
    for sa in addrs {
        match TcpStream::connect_timeout(&sa, MASTER_CONNECT_TIMEOUT) {
            Ok(stream) => {
                let _ = stream.shutdown(std::net::Shutdown::Both);
                return Ok(());
            }
            Err(e) => last_err = e.to_string(),
        }
    }
    Err(rusql_i18n::messages::sql_replica_bad_host(host, &last_err))
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
        let bytes = fs::read(&path).map_err(|_| {
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
        fs::create_dir_all(data_dir).map_err(|_| {
            StorageError::Message(rusql_i18n::messages::sql_replica_role_persist_failed())
        })?;
        let json = serde_json::to_vec_pretty(self).map_err(|_| {
            StorageError::Message(rusql_i18n::messages::sql_replica_role_persist_failed())
        })?;
        fs::write(data_dir.join(ROLE_FILE), json).map_err(|_| {
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
    use super::*;
    use crate::PersistentEngine;
    use std::net::TcpListener;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rusql-replica-{}-{}-{}",
            std::process::id(),
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn change_master_persists_and_reloads() {
        let dir = temp_dir("persist");
        let mut state = ReplicaState::load(&dir).unwrap();
        state
            .change_master(
                Some("127.0.0.1".into()),
                Some(3307),
                Some("repl".into()),
                Some("secret".into()),
                Some("binlog.000001".into()),
                Some(4),
            )
            .unwrap();
        let loaded = ReplicaState::load(&dir).unwrap();
        let cfg = loaded.config.unwrap();
        assert_eq!(cfg.master_host, "127.0.0.1");
        assert_eq!(cfg.master_port, 3307);
        assert_eq!(cfg.master_user, "repl");
        assert_eq!(cfg.master_password, "secret");
        assert_eq!(cfg.master_log_file.as_deref(), Some("binlog.000001"));
        assert_eq!(cfg.master_log_pos, Some(4));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_slave_requires_config() {
        let mut state = ReplicaState::new();
        match state.start_slave() {
            Err(ReplicaControlError::Mysql { code, message }) => {
                assert_eq!(code, ER_BAD_SLAVE);
                assert!(message.contains("not configured") || message.contains("CHANGE MASTER"));
            }
            other => panic!("expected errno 1200, got {other:?}"),
        }
    }

    #[test]
    fn start_slave_invalid_port_is_errno_1231() {
        let dir = temp_dir("bad_port");
        let mut state = ReplicaState::load(&dir).unwrap();
        match state.change_master(
            Some("127.0.0.1".into()),
            Some(0),
            Some("repl".into()),
            None,
            None,
            None,
        ) {
            Err(ReplicaControlError::Mysql { code, .. }) => {
                assert_eq!(code, ER_WRONG_VALUE_FOR_VAR)
            }
            other => panic!("expected errno 1231, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_slave_change_while_running_is_errno_1198() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let dir = temp_dir("running_change");
        let mut state = ReplicaState::load(&dir).unwrap();
        state
            .change_master(
                Some("127.0.0.1".into()),
                Some(port),
                Some("repl".into()),
                None,
                None,
                None,
            )
            .unwrap();
        state.start_slave().unwrap();
        match state.change_master(Some("10.0.0.1".into()), None, None, None, None, None) {
            Err(ReplicaControlError::Mysql { code, .. }) => assert_eq!(code, ER_SLAVE_MUST_STOP),
            other => panic!("expected errno 1198, got {other:?}"),
        }
        state.stop_slave();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_slave_bad_host_documented_error() {
        let dir = temp_dir("bad_host");
        let mut state = ReplicaState::load(&dir).unwrap();
        state
            .change_master(
                Some("127.0.0.1".into()),
                Some(1), // privileged / closed port
                Some("repl".into()),
                None,
                None,
                None,
            )
            .unwrap();
        match state.start_slave() {
            Err(ReplicaControlError::Mysql { code, message }) => {
                assert_eq!(code, CR_CONN_HOST_ERROR);
                assert!(!message.is_empty());
            }
            other => panic!("expected errno 2003, got {other:?}"),
        }
        assert!(!state.status.io_running);
        assert!(!state.status.sql_running);
        assert!(!state.status.last_io_error.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_slave_and_stop_slave() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let dir = temp_dir("start_stop");
        let mut state = ReplicaState::load(&dir).unwrap();
        state
            .change_master(
                Some("127.0.0.1".into()),
                Some(port),
                Some("repl".into()),
                None,
                None,
                None,
            )
            .unwrap();
        state.start_slave().unwrap();
        assert!(state.status.io_running);
        assert!(state.status.sql_running);
        assert!(state.status.dump_active);
        let (cols, row) = state.show_status_row("");
        assert!(cols.contains(&"Slave_IO_Running".into()));
        assert_eq!(row[3], "Yes");
        assert_eq!(row[4], "Yes");
        state.stop_slave();
        assert!(!state.status.io_running);
        assert!(!state.status.sql_running);
        assert!(!state.status.dump_active);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn probe_master_tcp_invalid_hostname() {
        let err = probe_master_tcp("this.host.definitely.invalid.rusql", 3306).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn replica_promote_persists_and_does_not_touch_other_directory() {
        let primary = temp_dir("promote-primary");
        let replica = temp_dir("promote-replica");

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

        let _ = fs::remove_dir_all(&primary);
        let _ = fs::remove_dir_all(&replica);
    }
}

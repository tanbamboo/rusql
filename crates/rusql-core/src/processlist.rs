//! Connection registry for `SHOW PROCESSLIST` / `COM_PROCESS_INFO`.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Instant;

/// One row in `SHOW PROCESSLIST`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessListRow {
    pub id: u64,
    pub user: String,
    pub host: String,
    pub db: String,
    pub command: String,
    pub time: u64,
    pub state: String,
    pub info: Option<String>,
}

#[derive(Debug, Clone)]
struct ConnectionInfo {
    id: u64,
    user: String,
    host: String,
    database: String,
    command: String,
    info: Option<String>,
    state: String,
    command_started: Instant,
}

/// Shared registry of active server connections.
#[derive(Debug, Default)]
pub struct ConnectionRegistry {
    inner: RwLock<HashMap<u64, ConnectionInfo>>,
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &self,
        id: u64,
        user: impl Into<String>,
        host: impl Into<String>,
        database: impl Into<String>,
    ) {
        let now = Instant::now();
        let info = ConnectionInfo {
            id,
            user: user.into(),
            host: host.into(),
            database: database.into(),
            command: "Sleep".into(),
            info: None,
            state: "".into(),
            command_started: now,
        };
        self.inner
            .write()
            .expect("connection registry lock")
            .insert(id, info);
    }

    pub fn unregister(&self, id: u64) {
        self.inner
            .write()
            .expect("connection registry lock")
            .remove(&id);
    }

    pub fn update_session(&self, id: u64, user: &str, host: &str, database: &str) {
        let mut guard = self.inner.write().expect("connection registry lock");
        if let Some(entry) = guard.get_mut(&id) {
            entry.user = user.to_string();
            entry.host = host.to_string();
            entry.database = database.to_string();
        }
    }

    pub fn set_command(&self, id: u64, command: &str, info: Option<&str>) {
        let mut guard = self.inner.write().expect("connection registry lock");
        if let Some(entry) = guard.get_mut(&id) {
            entry.command = command.to_string();
            entry.info = info.map(str::to_string);
            entry.command_started = Instant::now();
        }
    }

    pub fn set_sleep(&self, id: u64) {
        let mut guard = self.inner.write().expect("connection registry lock");
        if let Some(entry) = guard.get_mut(&id) {
            entry.command = "Sleep".into();
            entry.info = None;
            entry.state = "".into();
            entry.command_started = Instant::now();
        }
    }

    pub fn snapshot(&self) -> Vec<ProcessListRow> {
        let guard = self.inner.read().expect("connection registry lock");
        let mut rows: Vec<ProcessListRow> = guard.values().map(to_row).collect();
        rows.sort_by_key(|r| r.id);
        rows
    }

    pub fn current(&self, id: u64) -> Option<ProcessListRow> {
        let guard = self.inner.read().expect("connection registry lock");
        guard.get(&id).map(to_row)
    }
}

fn to_row(entry: &ConnectionInfo) -> ProcessListRow {
    ProcessListRow {
        id: entry.id,
        user: entry.user.clone(),
        host: entry.host.clone(),
        db: entry.database.clone(),
        command: entry.command.clone(),
        time: entry.command_started.elapsed().as_secs(),
        state: entry.state.clone(),
        info: entry.info.clone(),
    }
}

//! MySQL slow query log file (M191).
//!
//! A documented subset: one entry per finished `COM_QUERY` whose elapsed time is
//! at least `long_query_time`. Off by default, so the file is not created.
//! Not the general log (M192). `Slow_queries` stays the M190 stub.

use crate::audit::{redact_audit_command, timestamp_rfc3339};
use rusql_core::Session;
use rusql_executor::{effective_long_query_micros, SlowLogGlobals};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct SlowLogState {
    path: PathBuf,
    long_query_time: String,
    file: Option<File>,
}

/// Shared slow-query log for every connection on this process.
pub(crate) struct SlowQueryLog {
    enabled: AtomicBool,
    state: Mutex<SlowLogState>,
}

impl SlowQueryLog {
    /// `enabled` opens the file immediately so a bad path fails startup.
    /// Disabled leaves the filesystem untouched.
    pub(crate) fn open(
        path: &Path,
        enabled: bool,
        long_query_time: &str,
    ) -> std::io::Result<Arc<Self>> {
        let log = Arc::new(Self {
            enabled: AtomicBool::new(enabled),
            state: Mutex::new(SlowLogState {
                path: path.to_path_buf(),
                long_query_time: long_query_time.to_string(),
                file: None,
            }),
        });
        if enabled {
            log.ensure_file()?;
        }
        Ok(log)
    }

    pub(crate) fn globals(&self) -> SlowLogGlobals {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        SlowLogGlobals {
            enabled: self.enabled.load(Ordering::Relaxed),
            path: state.path.display().to_string(),
            long_query_time: state.long_query_time.clone(),
        }
    }

    /// Apply `SET GLOBAL` values captured on the session. A new path reopens on the next write.
    pub(crate) fn absorb(&self, globals: &SlowLogGlobals) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let next = PathBuf::from(&globals.path);
        if state.path != next {
            state.path = next;
            state.file = None;
        }
        state.long_query_time = globals.long_query_time.clone();
        drop(state);
        self.enabled.store(globals.enabled, Ordering::Relaxed);
    }

    /// Append when the log is on and `elapsed` is at least the session threshold.
    /// `long_query_time = 0` logs every non-empty statement. Passwords are redacted.
    /// A write error does not change the client result.
    pub(crate) fn maybe_append(&self, session: &Session, host: &str, sql: &str, elapsed: Duration) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let threshold = effective_long_query_micros(session);
        if elapsed.as_micros() < threshold {
            return;
        }
        let sql = redact_audit_command(sql);
        if sql.trim().is_empty() {
            return;
        }
        let line = format_entry(&timestamp_rfc3339(), &session.user, host, elapsed, &sql);
        if let Err(e) = self.write_line(&line) {
            tracing::warn!(error = %e, "slow query log append failed");
        }
    }

    fn write_line(&self, line: &str) -> std::io::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        open_state(&mut state)?;
        let Some(file) = state.file.as_mut() else {
            return Ok(());
        };
        writeln!(file, "{line}")?;
        file.flush()?;
        Ok(())
    }

    fn ensure_file(&self) -> std::io::Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        open_state(&mut state)
    }
}

fn open_state(state: &mut SlowLogState) -> std::io::Result<()> {
    if state.file.is_some() {
        return Ok(());
    }
    let path = state.path.clone();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    state.file = Some(OpenOptions::new().create(true).append(true).open(path)?);
    Ok(())
}

fn format_entry(time: &str, user: &str, host: &str, elapsed: Duration, sql: &str) -> String {
    let query_time = elapsed.as_secs_f64();
    format!("# Time: {time}\n# User@Host: {user} @ {host}\n# Query_time: {query_time:.6}\n{sql}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_protocol::client_decode::QueryResponse;

    #[test]
    fn slow_query_log_entry_has_time_user_and_query_time() {
        let line = format_entry(
            "2026-10-28T00:00:00.000Z",
            "root",
            "127.0.0.1",
            Duration::from_micros(12),
            "SELECT 1",
        );
        assert!(line.contains("# Time: 2026-10-28T00:00:00.000Z"));
        assert!(line.contains("# User@Host: root @ 127.0.0.1"));
        assert!(line.contains("# Query_time: 0.000012"));
        assert!(line.ends_with("SELECT 1"));
    }

    #[test]
    fn slow_query_log_disabled_does_not_create_a_file() {
        let dir = crate::test_support::temp_data_dir("slow-off-unit");
        let path = dir.join("rusql-slow.log");
        let log = SlowQueryLog::open(&path, false, "10.000000").unwrap();
        let session = Session::new(1, "root");
        log.maybe_append(&session, "127.0.0.1", "SELECT 1", Duration::from_secs(30));
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn slow_query_log_appends_select_when_threshold_is_zero() {
        let server = crate::test_support::TestServer::start("slow-query-log").await;
        let path = server.data_dir.join("rusql-slow.log");
        let mut client = server.connect().await;
        client.query("SELECT 1").await;
        assert!(
            !path.exists(),
            "disabled slow query log must not create a file"
        );
        assert!(matches!(
            client.query("SET GLOBAL slow_query_log = ON").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("SET SESSION long_query_time = 86400").await,
            QueryResponse::Ok { .. }
        ));
        client.query("SELECT 1").await;
        assert!(
            !path.exists(),
            "a fast SELECT under a high threshold must not be written"
        );
        assert!(matches!(
            client.query("SET SESSION long_query_time = 0").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT 42").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["42".to_string()]]);
            }
            other => panic!("select: {other:?}"),
        }
        let text = std::fs::read_to_string(&path).expect("slow log file");
        assert!(
            text.contains("SELECT 42"),
            "SELECT must be appended, got:\n{text}"
        );
        assert!(text.contains("# Query_time:"));
        assert!(text.contains("# User@Host: root @"));
        client.quit().await;
    }

    #[tokio::test]
    async fn slow_query_log_redacts_passwords_and_skips_change_user() {
        let server = crate::test_support::TestServer::start("slow-query-log-pw").await;
        let path = server.data_dir.join("rusql-slow.log");
        let mut client = server.connect().await;
        assert!(matches!(
            client.query("SET GLOBAL slow_query_log = ON").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client.query("SET SESSION long_query_time = 0").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("CREATE USER 'slowu'@'%' IDENTIFIED BY 'slow-identified-secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
        let _ = client.try_change_user("slowu", "slow-change-secret").await;
        client.query("SELECT 7").await;
        let text = std::fs::read_to_string(&path).expect("slow log file");
        assert!(
            !text.contains("slow-identified-secret"),
            "IDENTIFIED BY literal must be redacted:\n{text}"
        );
        assert!(text.contains("<redacted>"));
        assert!(
            !text.contains("slow-change-secret"),
            "COM_CHANGE_USER password must not be written:\n{text}"
        );
        assert!(text.contains("SELECT 7"));
        client.quit().await;
    }
}

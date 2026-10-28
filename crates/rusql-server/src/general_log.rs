//! MySQL general query log file (M192).
//!
//! A documented subset: one line per successful login (`Connect`), one line per
//! finished `COM_QUERY` (`Query`), and one line on `COM_QUIT` (`Quit`).
//! Off by default, so the file is not created. Result rows are not written.
//! Not the slow query log (M191) and not the M185 audit JSON file.

use crate::audit::{redact_audit_command, timestamp_rfc3339};
use rusql_executor::GeneralLogGlobals;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct GeneralLogState {
    path: PathBuf,
    file: Option<File>,
}

/// Shared general log for every connection on this process.
pub(crate) struct GeneralLog {
    enabled: AtomicBool,
    state: Mutex<GeneralLogState>,
}

impl GeneralLog {
    /// `enabled` opens the file immediately so a bad path fails startup.
    /// Disabled leaves the filesystem untouched.
    pub(crate) fn open(path: &Path, enabled: bool) -> std::io::Result<Arc<Self>> {
        let log = Arc::new(Self {
            enabled: AtomicBool::new(enabled),
            state: Mutex::new(GeneralLogState {
                path: path.to_path_buf(),
                file: None,
            }),
        });
        if enabled {
            log.ensure_file()?;
        }
        Ok(log)
    }

    pub(crate) fn globals(&self) -> GeneralLogGlobals {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        GeneralLogGlobals {
            enabled: self.enabled.load(Ordering::Relaxed),
            path: state.path.display().to_string(),
        }
    }

    /// Apply `SET GLOBAL` values captured on the session. A new path reopens on the next write.
    pub(crate) fn absorb(&self, globals: &GeneralLogGlobals) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let next = PathBuf::from(&globals.path);
        if state.path != next {
            state.path = next;
            state.file = None;
        }
        drop(state);
        self.enabled.store(globals.enabled, Ordering::Relaxed);
    }

    /// Login line. No password and no result rows. A write error does not fail the handshake.
    pub(crate) fn record_connect(
        &self,
        connection_id: u64,
        user: &str,
        host: &str,
        database: &str,
    ) {
        let argument = if database.is_empty() {
            format!("{user}@{host}")
        } else {
            format!("{user}@{host} on {database}")
        };
        self.append(connection_id, "Connect", &argument);
    }

    /// Finished `COM_QUERY` text on one line. Passwords are redacted. Result rows are not written.
    pub(crate) fn record_query(&self, connection_id: u64, sql: &str) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let sql = redact_audit_command(sql);
        if sql.trim().is_empty() {
            return;
        }
        self.append(connection_id, "Query", &sql);
    }

    /// Clean `COM_QUIT`. An abrupt disconnect is not a line.
    pub(crate) fn record_quit(&self, connection_id: u64) {
        self.append(connection_id, "Quit", "");
    }

    fn append(&self, connection_id: u64, command: &str, argument: &str) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let line = format_line(&timestamp_rfc3339(), connection_id, command, argument);
        if let Err(e) = self.write_line(&line) {
            tracing::warn!(error = %e, "general log append failed");
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

fn open_state(state: &mut GeneralLogState) -> std::io::Result<()> {
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

fn format_line(time: &str, id: u64, command: &str, argument: &str) -> String {
    if argument.is_empty() {
        format!("{time}\t{id}\t{command}")
    } else {
        format!("{time}\t{id}\t{command}\t{argument}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_protocol::client_decode::QueryResponse;
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn general_log_line_has_time_id_command_and_argument() {
        let line = format_line("2026-10-28T00:00:00.000Z", 8, "Query", "SELECT 1");
        assert!(line.contains("2026-10-28T00:00:00.000Z"));
        assert!(line.contains("\t8\tQuery\tSELECT 1"));
        let quit = format_line("2026-10-28T00:00:00.000Z", 8, "Quit", "");
        assert!(quit.ends_with("\t8\tQuit"));
        assert!(!quit.contains("SELECT"));
    }

    #[test]
    fn general_log_disabled_does_not_create_a_file() {
        let dir = crate::test_support::temp_data_dir("general-off-unit");
        let path = dir.join("rusql-general.log");
        let log = GeneralLog::open(&path, false).unwrap();
        log.record_connect(1, "root", "127.0.0.1", "rusql");
        log.record_query(1, "SELECT 1");
        log.record_quit(1);
        assert!(!path.exists());
    }

    #[test]
    fn general_log_enabled_writes_connect_query_and_redacts_passwords() {
        let dir = crate::test_support::temp_data_dir("general-on-unit");
        let path = dir.join("rusql-general.log");
        let log = GeneralLog::open(&path, true).unwrap();
        log.record_connect(3, "root", "127.0.0.1", "rusql");
        log.record_query(3, "CREATE USER 'a'@'%' IDENTIFIED BY 'unit-general-secret'");
        log.record_quit(3);
        let text = std::fs::read_to_string(&path).expect("general log");
        assert!(text.contains("Connect"));
        assert!(text.contains("root@127.0.0.1 on rusql"));
        assert!(text.contains("Query"));
        assert!(text.contains("CREATE USER"));
        assert!(text.contains("<redacted>"));
        assert!(!text.contains("unit-general-secret"), "{text}");
        assert!(text.contains("Quit"));
    }

    #[tokio::test]
    async fn general_log_appends_connect_and_query_when_enabled() {
        let server = crate::test_support::TestServer::start("general-log").await;
        let path = server.data_dir.join("rusql-general.log");
        let mut client = server.connect().await;
        assert!(matches!(
            client.query("CREATE TABLE gl_m192 (c VARCHAR(64))").await,
            QueryResponse::Ok { .. }
        ));
        assert!(matches!(
            client
                .query("INSERT INTO gl_m192 VALUES ('cell-only-secret')")
                .await,
            QueryResponse::Ok { .. }
        ));
        client.query("SELECT 1").await;
        assert!(
            !path.exists(),
            "disabled general log must not create a file"
        );
        assert!(matches!(
            client.query("SET GLOBAL general_log = ON").await,
            QueryResponse::Ok { .. }
        ));
        match client.query("SELECT c FROM gl_m192").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["cell-only-secret".to_string()]]);
            }
            other => panic!("select: {other:?}"),
        }
        let text = wait_for_log(&path, |t| t.contains("SELECT c FROM gl_m192")).await;
        assert!(text.contains("\tQuery\t"), "query command missing:\n{text}");
        assert!(
            text.contains("SET GLOBAL general_log = ON"),
            "enabling statement should be logged once the log is on:\n{text}"
        );
        assert!(
            !text.contains("cell-only-secret"),
            "result cell must not be written:\n{text}"
        );

        let mut second = server.connect().await;
        second.query("SELECT 99").await;
        let text = wait_for_log(&path, |t| t.contains("SELECT 99") && t.contains("Connect")).await;
        assert!(
            text.contains("root@127.0.0.1"),
            "connect line missing:\n{text}"
        );
        assert!(
            text.contains("\tConnect\t"),
            "connect command missing:\n{text}"
        );

        assert!(matches!(
            client
                .query("CREATE USER 'glu'@'%' IDENTIFIED BY 'general-identified-secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
        let _ = client.try_change_user("glu", "general-change-secret").await;
        client.query("SELECT 7").await;
        let text = wait_for_log(&path, |t| t.contains("SELECT 7")).await;
        assert!(
            !text.contains("general-identified-secret"),
            "IDENTIFIED BY literal must be redacted:\n{text}"
        );
        assert!(text.contains("<redacted>"));
        assert!(
            !text.contains("general-change-secret"),
            "COM_CHANGE_USER password must not be written:\n{text}"
        );
        assert!(
            !text.contains("cell-only-secret"),
            "result cell must stay out of the file:\n{text}"
        );

        let other = server.data_dir.join("other-general.log");
        let shown = other.display().to_string().replace('\\', "/");
        assert!(matches!(
            client
                .query(&format!("SET GLOBAL general_log_file = '{shown}'"))
                .await,
            QueryResponse::Ok { .. }
        ));
        client.query("SELECT 8").await;
        let moved = wait_for_log(&other, |t| t.contains("SELECT 8")).await;
        assert!(moved.contains("SELECT 8"), "{moved}");
        let original = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            !original.contains("SELECT 8"),
            "a new path must not keep appending the old file:\n{original}"
        );

        client.quit().await;
        let text = wait_for_log(&other, |t| t.contains("Quit")).await;
        assert!(text.contains("\tQuit"), "quit line missing:\n{text}");
        second.quit().await;
    }

    async fn wait_for_log(path: &Path, pred: impl Fn(&str) -> bool) -> String {
        let mut last = String::new();
        for _ in 0..50 {
            if let Ok(text) = std::fs::read_to_string(path) {
                if pred(&text) {
                    return text;
                }
                last = text;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        last
    }
}

//! MySQL error log file (M195).
//!
//! Off unless `--log-error` / `--log_error` names a path. The file then receives
//! one startup line and server errors: a failed login, each SQL ERR packet, and
//! a startup failure. Stderr tracing is left as it was. Passwords are not
//! written. This slice does not rotate the file and does not implement
//! `log_error_verbosity`.

use crate::audit::{redact_audit_command, timestamp_rfc3339};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Append-only error log shared by every connection on this process.
pub(crate) struct ErrorLog {
    path: PathBuf,
    file: Mutex<File>,
}

impl ErrorLog {
    /// Create or append `path`. Parent directories are created when the path has one.
    pub(crate) fn open(path: &Path) -> std::io::Result<Arc<Self>> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Arc::new(Self {
            path: path.to_path_buf(),
            file: Mutex::new(file),
        }))
    }

    /// Path stored in `@@log_error` (the path passed to [`ErrorLog::open`]).
    pub(crate) fn displayed_path(&self) -> String {
        self.path.display().to_string()
    }

    /// One startup line. The message is already localized.
    pub(crate) fn record_startup(&self, message: &str) {
        self.write_line(&format_system_line(&timestamp_rfc3339(), message));
    }

    /// One error line. Password literals in `message` are redacted. A write
    /// error does not change the client result.
    pub(crate) fn record_error(&self, connection_id: u64, errno: u16, message: &str) {
        let message = redact_audit_command(message);
        self.write_line(&format_error_line(
            &timestamp_rfc3339(),
            connection_id,
            errno,
            &message,
        ));
    }

    fn write_line(&self, line: &str) {
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = writeln!(file, "{line}") {
            tracing::warn!(error = %e, "error log append failed");
            return;
        }
        if let Err(e) = file.flush() {
            tracing::warn!(error = %e, "error log flush failed");
        }
    }
}

/// `System` is a file-format token in every locale. Connection id is 0.
fn format_system_line(time: &str, message: &str) -> String {
    format!("{time}\t0\tSystem\t{message}")
}

/// `Error` is a file-format token in every locale.
fn format_error_line(time: &str, id: u64, errno: u16, message: &str) -> String {
    format!("{time}\t{id}\tError\t{errno}\t{message}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_protocol::client_decode::QueryResponse;
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn log_error_line_has_severity_and_redacts_passwords() {
        let system = format_system_line("2026-10-28T00:00:00.000Z", "listening");
        assert!(system.contains("2026-10-28T00:00:00.000Z"));
        assert!(system.contains("\t0\tSystem\tlistening"));
        let err = format_error_line("2026-10-28T00:00:00.000Z", 8, 1146, "missing");
        assert!(err.contains("\t8\tError\t1146\tmissing"));

        let dir = crate::test_support::temp_data_dir("log-error-unit");
        let path = dir.join("rusql.err");
        let log = ErrorLog::open(&path).unwrap();
        log.record_startup("ready");
        log.record_error(
            4,
            1064,
            "CREATE USER 'a'@'%' IDENTIFIED BY 'unit-error-secret'",
        );
        let text = std::fs::read_to_string(&path).expect("error log");
        assert!(text.contains("\tSystem\tready"), "{text}");
        assert!(text.contains("\tError\t1064\t"), "{text}");
        assert!(text.contains("<redacted>"), "{text}");
        assert!(!text.contains("unit-error-secret"), "{text}");
    }

    #[test]
    fn log_error_disabled_does_not_create_a_file() {
        let dir = crate::test_support::temp_data_dir("log-error-off-unit");
        let path = dir.join("rusql.err");
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn log_error_off_does_not_write_sql_errors() {
        let server = crate::test_support::TestServer::start("log-error-off").await;
        let path = server.data_dir.join("rusql.err");
        let mut client = server.connect().await;
        assert!(matches!(
            client.query("SELECT * FROM no_such_m195_off").await,
            QueryResponse::Err { .. }
        ));
        assert!(
            !path.exists(),
            "omitting --log-error must not create an error log file"
        );
        match client.query("SELECT @@log_error").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(
                    rows,
                    vec![vec![rusql_executor::LOG_ERROR_STDERR.to_string()]]
                );
            }
            other => panic!("@@log_error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn log_error_appends_startup_failed_login_and_sql_error() {
        let dir = crate::test_support::temp_data_dir("log-error-on");
        let path = dir.join("rusql.err");
        let server =
            crate::test_support::TestServer::start_with_error_log("log-error-on", path.clone())
                .await;
        let mut client = server.connect().await;
        assert!(matches!(
            client
                .query("CREATE USER 'm195'@'%' IDENTIFIED BY 'unit-error-secret'")
                .await,
            QueryResponse::Ok { .. }
        ));
        let failed = server.try_connect_as("m195", "wrong-secret").await;
        assert!(
            failed.is_err(),
            "wrong password must fail login: {failed:?}"
        );
        assert!(matches!(
            client.query("SELECT * FROM no_such_m195").await,
            QueryResponse::Err { code: 1146, .. }
        ));
        assert!(matches!(
            client
                .query("CREATE USER 'b'@'%' IDENTIFIED BY 'unit-error-secret' BOGUS")
                .await,
            QueryResponse::Err { .. }
        ));
        assert!(matches!(
            client.query("SELECT 1").await,
            QueryResponse::Rows { .. }
        ));

        let text = wait_for_log(&path, |t| {
            t.contains("\tError\t1146\t") && t.contains("\tError\t1045\t")
        })
        .await;
        assert!(text.contains("\tSystem\t"), "startup line missing:\n{text}");
        assert!(
            text.contains(&server.addr.port().to_string()),
            "startup line should name the listen port:\n{text}"
        );
        assert!(
            text.contains("\tError\t1045\t"),
            "failed login missing:\n{text}"
        );
        assert!(
            text.contains("\tError\t1146\t"),
            "sql error missing:\n{text}"
        );
        assert!(!text.contains("unit-error-secret"), "{text}");
        assert!(!text.contains("wrong-secret"), "{text}");
        assert!(
            !text.contains("SELECT 1"),
            "a successful statement is not an error-log line:\n{text}"
        );

        match client.query("SELECT @@log_error").await {
            QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows.len(), 1);
                assert!(
                    rows[0][0].contains("rusql.err"),
                    "@@log_error should be the file path, got {}",
                    rows[0][0]
                );
            }
            other => panic!("@@log_error: {other:?}"),
        }
        match client.query("SELECT @@global.log_error").await {
            QueryResponse::Rows { rows, .. } => {
                assert!(rows[0][0].contains("rusql.err"), "{}", rows[0][0]);
            }
            other => panic!("@@global.log_error: {other:?}"),
        }
        match client.query("SET GLOBAL log_error = 'x'").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1238),
            other => panic!("expected read-only errno 1238, got {other:?}"),
        }
        match client.query("SET log_error = 'x'").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1229),
            other => panic!("expected errno 1229, got {other:?}"),
        }
        match client.query("SET SESSION log_error = 'x'").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1229),
            other => panic!("expected errno 1229, got {other:?}"),
        }
        match client.query("SELECT @@session.log_error").await {
            QueryResponse::Err { code, .. } => assert_eq!(code, 1238),
            other => panic!("expected errno 1238, got {other:?}"),
        }
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

//! Opt-in JSON-lines audit file (M185).
//!
//! Not the Oracle MySQL Enterprise Audit plugin. The file records connect,
//! COM_QUERY, and disconnect events. It does not store result rows or
//! authentication packets.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

const REDACTED: &str = "'<redacted>'";

/// Append-only audit log shared by every connection.
pub(crate) struct AuditLog {
    file: Mutex<File>,
}

impl AuditLog {
    /// Create or append `path`. Parent directories are created when the path has one.
    pub(crate) fn open(path: &Path) -> std::io::Result<Arc<Self>> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Arc::new(Self {
            file: Mutex::new(file),
        }))
    }

    /// One JSON object on its own line: timestamp, user, command, status.
    pub(crate) fn append(&self, user: &str, command: &str, status: u16) {
        let line = format_event(&timestamp_rfc3339(), user, command, status);
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = writeln!(file, "{line}") {
            tracing::warn!(error = %e, "audit log append failed");
            return;
        }
        if let Err(e) = file.flush() {
            tracing::warn!(error = %e, "audit log flush failed");
        }
    }
}

/// Writes `Quit` when the accepted session ends, including on task drop.
pub(crate) struct DisconnectAudit {
    log: Arc<AuditLog>,
    user: Arc<Mutex<String>>,
}

impl DisconnectAudit {
    pub(crate) fn arm(log: Arc<AuditLog>, user: Arc<Mutex<String>>) -> Self {
        Self { log, user }
    }
}

impl Drop for DisconnectAudit {
    fn drop(&mut self) {
        let user = self.user.lock().unwrap_or_else(|e| e.into_inner()).clone();
        self.log.append(&user, "Quit", 0);
    }
}

/// Counts the first MySQL ERR packet written on this connection.
///
/// An ERR payload is `0xFF`, a little-endian errno, `#`, and a 5-character SQLSTATE.
/// Result rows are not treated as errors.
pub(crate) struct ErrWatch<S> {
    inner: S,
    buf: Vec<u8>,
    errno: Arc<AtomicU16>,
}

impl<S> ErrWatch<S> {
    pub(crate) fn new(inner: S, errno: Arc<AtomicU16>) -> Self {
        Self {
            inner,
            buf: Vec::new(),
            errno,
        }
    }
}

impl<S> AsyncRead for ErrWatch<S>
where
    S: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
    }
}

impl<S> AsyncWrite for ErrWatch<S>
where
    S: AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = result {
            observe(&mut this.buf, &this.errno, &buf[..n]);
            return Poll::Ready(Ok(n));
        }
        result
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

fn observe(buf: &mut Vec<u8>, errno: &AtomicU16, bytes: &[u8]) {
    buf.extend_from_slice(bytes);
    loop {
        if buf.len() < 4 {
            return;
        }
        let len = u32::from_le_bytes([buf[0], buf[1], buf[2], 0]) as usize;
        if buf.len() < 4 + len {
            return;
        }
        let code = err_code(&buf[4..4 + len]);
        buf.drain(..4 + len);
        if let Some(code) = code {
            if code != 0 && errno.load(Ordering::Relaxed) == 0 {
                errno.store(code, Ordering::Relaxed);
            }
        }
    }
}

fn err_code(payload: &[u8]) -> Option<u16> {
    if payload.len() >= 9
        && payload[0] == 0xFF
        && payload[3] == b'#'
        && payload[4..9].iter().all(|b| b.is_ascii_alphanumeric())
    {
        Some(u16::from_le_bytes([payload[1], payload[2]]))
    } else {
        None
    }
}

/// Collapse the statement to one physical line and replace password literals.
pub(crate) fn redact_audit_command(sql: &str) -> String {
    redact_passwords(&flatten_line(sql))
}

fn flatten_line(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_space = false;
    for c in input.chars() {
        if c == '\n' || c == '\r' || c == '\t' {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else if c.is_control() {
            continue;
        } else {
            prev_space = c == ' ';
            out.push(c);
        }
    }
    out
}

fn redact_passwords(sql: &str) -> String {
    let lower = sql.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' || bytes[i] == b'"' || bytes[i] == b'`' {
            let end = end_of_quote(sql, i);
            out.push_str(&sql[i..end]);
            i = end;
            continue;
        }
        if let Some(marker_end) = marker_at(&lower, i) {
            out.push_str(&sql[i..marker_end]);
            if let Some(lit) = next_literal(&sql[marker_end..]) {
                out.push_str(&sql[marker_end..marker_end + lit.leading_ws]);
                out.push_str(REDACTED);
                i = marker_end + lit.leading_ws + lit.len;
            } else {
                i = marker_end;
            }
            continue;
        }
        let ch = sql[i..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

struct Literal {
    leading_ws: usize,
    len: usize,
}

fn marker_at(lower: &str, i: usize) -> Option<usize> {
    if let Some(end) = match_word(lower, i, "identified") {
        return after_identified(lower, end);
    }
    if let Some(end) = match_word(lower, i, "set") {
        return after_set_password(lower, end);
    }
    if let Some(end) = match_word(lower, i, "password") {
        let j = skip_ws(lower, end);
        if lower.as_bytes().get(j) == Some(&b'(') {
            return Some(j + 1);
        }
    }
    None
}

fn after_identified(lower: &str, end: usize) -> Option<usize> {
    let j = skip_ws(lower, end);
    if let Some(by_end) = match_word(lower, j, "by") {
        let j = skip_ws(lower, by_end);
        if let Some(pw_end) = match_word(lower, j, "password") {
            return Some(skip_ws(lower, pw_end));
        }
        return Some(j);
    }
    if let Some(with_end) = match_word(lower, j, "with") {
        let mut j = skip_token(lower, skip_ws(lower, with_end));
        j = skip_ws(lower, j);
        if let Some(k) = match_word(lower, j, "by").or_else(|| match_word(lower, j, "as")) {
            return Some(skip_ws(lower, k));
        }
    }
    None
}

fn after_set_password(lower: &str, end: usize) -> Option<usize> {
    let j = skip_ws(lower, end);
    let mut i = match_word(lower, j, "password")?;
    let bytes = lower.as_bytes();
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        if let Some(q) = quote {
            if bytes[i] == b'\\' && q != b'`' {
                i = (i + 2).min(bytes.len());
                continue;
            }
            if bytes[i] == q {
                if q == b'\'' && bytes.get(i + 1) == Some(&b'\'') {
                    i += 2;
                    continue;
                }
                quote = None;
            }
            i += 1;
            continue;
        }
        match bytes[i] {
            b'\'' | b'"' | b'`' => {
                quote = Some(bytes[i]);
                i += 1;
            }
            b'=' => return Some(i + 1),
            b';' => return None,
            _ => i += 1,
        }
    }
    None
}

fn match_word(lower: &str, i: usize, word: &str) -> Option<usize> {
    let bytes = lower.as_bytes();
    if i > 0 {
        let prev = bytes[i - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return None;
        }
    }
    let end = i + word.len();
    if bytes.len() < end || &lower[i..end] != word {
        return None;
    }
    if end < bytes.len() {
        let next = bytes[end];
        if next.is_ascii_alphanumeric() || next == b'_' {
            return None;
        }
    }
    Some(end)
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    let bytes = s.as_bytes();
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn skip_token(s: &str, i: usize) -> usize {
    let bytes = s.as_bytes();
    if i >= bytes.len() {
        return i;
    }
    if bytes[i] == b'\'' || bytes[i] == b'"' || bytes[i] == b'`' {
        return end_of_quote(s, i);
    }
    let mut j = i;
    while j < bytes.len()
        && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'.')
    {
        j += 1;
    }
    j
}

fn end_of_quote(s: &str, i: usize) -> usize {
    let bytes = s.as_bytes();
    let q = bytes[i];
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == b'\\' && q != b'`' {
            j = (j + 2).min(bytes.len());
            continue;
        }
        if bytes[j] == q {
            if q == b'\'' && bytes.get(j + 1) == Some(&b'\'') {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    s.len()
}

fn next_literal(s: &str) -> Option<Literal> {
    let bytes = s.as_bytes();
    let mut ws = 0;
    while ws < bytes.len() && bytes[ws].is_ascii_whitespace() {
        ws += 1;
    }
    if ws >= bytes.len() {
        return None;
    }
    let q = bytes[ws];
    if q == b'\'' || q == b'"' {
        let end = end_of_quote(s, ws);
        if end <= ws + 1 || !s.is_char_boundary(end) {
            return None;
        }
        if end == s.len() && bytes[end - 1] != q {
            return None;
        }
        return Some(Literal {
            leading_ws: ws,
            len: end - ws,
        });
    }
    if q == b'*' {
        let mut j = ws + 1;
        while j < bytes.len() && bytes[j].is_ascii_hexdigit() {
            j += 1;
        }
        if j > ws + 1 {
            return Some(Literal {
                leading_ws: ws,
                len: j - ws,
            });
        }
    }
    None
}

fn format_event(timestamp: &str, user: &str, command: &str, status: u16) -> String {
    format!(
        "{{\"timestamp\":{},\"user\":{},\"command\":{},\"status\":{status}}}",
        json_string(timestamp),
        json_string(user),
        json_string(command),
    )
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn timestamp_rfc3339() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format_unix(dur.as_secs(), dur.subsec_millis())
}

fn format_unix(secs: u64, millis: u32) -> String {
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    let hh = tod / 3_600;
    let mm = (tod % 3_600) / 60;
    let ss = tod % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{millis:03}Z")
}

/// Howard Hinnant `civil_from_days` (days since Unix epoch).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    (y, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_protocol::err_packet;

    #[test]
    fn unix_epoch_formats_as_rfc3339() {
        assert_eq!(format_unix(0, 0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_unix(86_400, 0), "1970-01-02T00:00:00.000Z");
        assert_eq!(format_unix(86_400 + 3_661, 7), "1970-01-02T01:01:01.007Z");
    }

    #[test]
    fn audit_log_redacts_password_literals_only() {
        let cases = [
            (
                "CREATE USER 'app'@'%' IDENTIFIED BY 'pw-should-not-appear'",
                "CREATE USER 'app'@'%' IDENTIFIED BY '<redacted>'",
            ),
            (
                "ALTER USER 'app'@'%' IDENTIFIED WITH caching_sha2_password BY 'pw-should-not-appear'",
                "ALTER USER 'app'@'%' IDENTIFIED WITH caching_sha2_password BY '<redacted>'",
            ),
            (
                "CREATE USER 'app'@'%' IDENTIFIED BY PASSWORD '*ABCD1234'",
                "CREATE USER 'app'@'%' IDENTIFIED BY PASSWORD '<redacted>'",
            ),
            (
                "ALTER USER 'app'@'%' IDENTIFIED WITH mysql_native_password AS '*ABCD'",
                "ALTER USER 'app'@'%' IDENTIFIED WITH mysql_native_password AS '<redacted>'",
            ),
            (
                "SET PASSWORD FOR 'app'@'%' = 'pw-should-not-appear'",
                "SET PASSWORD FOR 'app'@'%' = '<redacted>'",
            ),
            (
                "SET PASSWORD = PASSWORD('pw-should-not-appear')",
                "SET PASSWORD = PASSWORD('<redacted>')",
            ),
            (
                "SELECT PASSWORD('pw-should-not-appear')",
                "SELECT PASSWORD('<redacted>')",
            ),
            (
                "SELECT 'pw-should-not-appear'",
                "SELECT 'pw-should-not-appear'",
            ),
            ("SELECT password FROM t", "SELECT password FROM t"),
            (
                "INSERT INTO t VALUES ('audit_secret_row_value')",
                "INSERT INTO t VALUES ('audit_secret_row_value')",
            ),
        ];
        for (input, expect) in cases {
            let got = redact_audit_command(input);
            assert_eq!(got, expect, "input {input}");
            if input.contains("IDENTIFIED")
                || input.contains("SET PASSWORD")
                || input.contains("PASSWORD(")
            {
                assert!(
                    !got.contains("pw-should-not-appear") && !got.contains("*ABCD"),
                    "password leaked: {got}"
                );
            }
        }
        assert_eq!(redact_audit_command("SELECT\n1"), "SELECT 1");
        let utf8 = redact_audit_command("CREATE USER 'u'@'%' IDENTIFIED BY '密码secret'");
        assert!(!utf8.contains('密'));
        assert!(utf8.contains("<redacted>"));
    }

    #[test]
    fn audit_log_line_is_json_with_four_fields() {
        let line = format_event("1970-01-01T00:00:00.000Z", "root", "SELECT 1", 0);
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["timestamp"], "1970-01-01T00:00:00.000Z");
        assert_eq!(value["user"], "root");
        assert_eq!(value["command"], "SELECT 1");
        assert_eq!(value["status"], 0);
        assert!(!line.contains('\n'));
    }

    #[test]
    fn err_watch_reads_mysql_errno_not_ok_or_rows() {
        let errno = AtomicU16::new(0);
        let mut buf = Vec::new();
        observe(&mut buf, &errno, &frame(&err_packet(1146, "missing")));
        assert_eq!(errno.load(Ordering::Relaxed), 1146);
        observe(&mut buf, &errno, &frame(&err_packet(1064, "later")));
        assert_eq!(errno.load(Ordering::Relaxed), 1146, "first errno wins");
        errno.store(0, Ordering::Relaxed);
        observe(
            &mut buf,
            &errno,
            &frame(&[0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00]),
        );
        observe(&mut buf, &errno, &frame(b"\x05hello"));
        assert_eq!(errno.load(Ordering::Relaxed), 0);
        assert!(buf.is_empty());
    }

    fn frame(payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + payload.len());
        let len = payload.len() as u32;
        out.extend_from_slice(&len.to_le_bytes()[..3]);
        out.push(0);
        out.extend_from_slice(payload);
        out
    }

    #[tokio::test]
    async fn audit_log_records_connect_query_and_quit() {
        let path = crate::test_support::temp_data_dir("audit-log").join("audit.jsonl");
        let log = AuditLog::open(&path).unwrap();
        let server = crate::test_support::TestServer::start_with_audit_log("audit-log", log).await;
        let mut client = server.connect().await;
        client
            .query("CREATE TABLE audit_t (id INT, secret VARCHAR(64))")
            .await;
        client
            .query("INSERT INTO audit_t VALUES (1, 'audit_secret_row_value')")
            .await;
        match client.query("SELECT secret FROM audit_t").await {
            rusql_protocol::client_decode::QueryResponse::Rows { rows, .. } => {
                assert_eq!(rows, vec![vec!["audit_secret_row_value".to_string()]]);
            }
            other => panic!("select: {other:?}"),
        }
        client
            .query("CREATE USER 'aud'@'%' IDENTIFIED BY 'pw-should-not-appear'")
            .await;
        match client.query("SELECT * FROM missing_audit_table_m185").await {
            rusql_protocol::client_decode::QueryResponse::Err { code, .. } => {
                assert_eq!(code, 1146);
            }
            other => panic!("missing table: {other:?}"),
        }
        client.query("SELECT 1").await;
        client.quit().await;
        drop(client);

        let events = wait_for_quit(&path).await;
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("pw-should-not-appear"),
            "password packet or literal in audit file"
        );
        assert_eq!(
            text.matches("audit_secret_row_value").count(),
            1,
            "result row must not be copied; the value may appear only in the INSERT statement"
        );

        assert_eq!(events[0]["command"], "Connect");
        assert_eq!(events[0]["user"], "root");
        assert_eq!(events[0]["status"], 0);
        assert!(events[0]["timestamp"].as_str().unwrap().ends_with('Z'));

        let queries: Vec<_> = events
            .iter()
            .filter(|e| e["command"] != "Connect" && e["command"] != "Quit")
            .collect();
        assert!(
            queries
                .iter()
                .any(|e| { e["command"] == "SELECT 1" && e["status"] == 0 && e["user"] == "root" }),
            "successful COM_QUERY missing: {events:?}"
        );
        let select = queries
            .iter()
            .find(|e| e["command"] == "SELECT secret FROM audit_t")
            .expect("select statement");
        assert_eq!(select["status"], 0);
        assert!(!select["command"]
            .as_str()
            .unwrap()
            .contains("audit_secret_row_value"));
        let missing = queries
            .iter()
            .find(|e| {
                e["command"]
                    .as_str()
                    .unwrap()
                    .contains("missing_audit_table_m185")
            })
            .expect("failed query");
        assert_eq!(missing["status"], 1146);
        let user = queries
            .iter()
            .find(|e| e["command"].as_str().unwrap().contains("CREATE USER"))
            .expect("create user");
        assert!(user["command"].as_str().unwrap().contains("<redacted>"));
        assert_eq!(user["status"], 0);

        let quit = events.last().unwrap();
        assert_eq!(quit["command"], "Quit");
        assert_eq!(quit["status"], 0);
        assert_eq!(quit["user"], "root");
        assert_eq!(
            events.iter().filter(|e| e["command"] == "SELECT 1").count(),
            1
        );
    }

    #[tokio::test]
    async fn audit_log_disabled_creates_no_file() {
        let absent = crate::test_support::temp_data_dir("audit-off").join("audit.jsonl");
        let server = crate::test_support::TestServer::start("audit-off").await;
        let mut client = server.connect().await;
        client.query("SELECT 1").await;
        client.quit().await;
        drop(client);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!absent.exists());
        assert!(!server.data_dir.join("audit.jsonl").exists());
    }

    async fn wait_for_quit(path: &Path) -> Vec<serde_json::Value> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Ok(text) = std::fs::read_to_string(path) {
                let mut events = Vec::new();
                let mut complete = true;
                for line in text.lines().filter(|line| !line.is_empty()) {
                    match serde_json::from_str::<serde_json::Value>(line) {
                        Ok(value) => events.push(value),
                        Err(_) => {
                            complete = false;
                            break;
                        }
                    }
                }
                if complete && events.iter().any(|event| event["command"] == "Quit") {
                    return events;
                }
            }
            if tokio::time::Instant::now() > deadline {
                let text = std::fs::read_to_string(path).unwrap_or_default();
                panic!("audit log missing Quit: {text}");
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}

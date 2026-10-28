//! `SHOW STATUS` catalog (M86 stubs + M190 live Questions / Uptime / Threads).
//!
//! Not live InnoDB / `performance_schema` counters. `SHOW STATUS`,
//! `SHOW SESSION STATUS`, and `SHOW GLOBAL STATUS` share one process-wide
//! list. When a connection registry is attached, `Questions` is the COM_QUERY
//! count, `Uptime` is whole seconds since process start, and
//! `Threads_connected` is the registry size. Other names stay M86 constants.

use crate::QueryResult;
use rusql_core::Session;
use rusql_storage::Row;
use sqlparser::ast::ShowStatementFilter;

/// Documented stub names (alphabetical, as `SHOW STATUS` lists them).
pub(crate) const STUB_NAMES: &[&str] = &[
    "Aborted_connects",
    "Bytes_received",
    "Bytes_sent",
    "Connections",
    "Open_tables",
    "Questions",
    "Slow_queries",
    "Threads_connected",
    "Threads_running",
    "Uptime",
];

const STUB_ABORTED_CONNECTS: &str = "0";
const STUB_BYTES_RECEIVED: &str = "0";
const STUB_BYTES_SENT: &str = "0";
const STUB_CONNECTIONS: &str = "1";
const STUB_OPEN_TABLES: &str = "0";
const STUB_QUESTIONS: &str = "0";
const STUB_SLOW_QUERIES: &str = "0";
const STUB_THREADS_CONNECTED_DEFAULT: &str = "1";
const STUB_THREADS_RUNNING: &str = "1";
const STUB_UPTIME: &str = "0";

/// Live values read from the session process list when present.
struct LiveStatus {
    threads_connected: String,
    questions: String,
    uptime: String,
}

/// `SHOW [SESSION|GLOBAL] STATUS [LIKE …]` over the documented catalog.
pub(crate) fn show_status(filter: Option<&ShowStatementFilter>, session: &Session) -> QueryResult {
    let pattern = filter.and_then(|f| match f {
        ShowStatementFilter::Like(p)
        | ShowStatementFilter::ILike(p)
        | ShowStatementFilter::NoKeyword(p) => Some(p.as_str()),
        ShowStatementFilter::Where(_) => None,
    });
    let live = live_status(session);
    let mut rows: Vec<Row> = STUB_NAMES
        .iter()
        .filter_map(|name| {
            if let Some(pat) = pattern {
                if !like_ci(name, pat) {
                    return None;
                }
            }
            Some(vec![(*name).to_string(), status_value(name, &live)])
        })
        .collect();
    rows.sort_by(|a, b| a[0].cmp(&b[0]));
    QueryResult::Rows {
        columns: vec!["Variable_name".into(), "Value".into()],
        rows,
    }
}

fn live_status(session: &Session) -> LiveStatus {
    match session.process_list.as_ref() {
        Some(reg) => LiveStatus {
            threads_connected: reg.connection_count().to_string(),
            questions: reg.questions().to_string(),
            uptime: reg.uptime_secs().to_string(),
        },
        None => LiveStatus {
            threads_connected: STUB_THREADS_CONNECTED_DEFAULT.to_string(),
            questions: STUB_QUESTIONS.to_string(),
            uptime: STUB_UPTIME.to_string(),
        },
    }
}

fn status_value(name: &str, live: &LiveStatus) -> String {
    match name {
        "Aborted_connects" => STUB_ABORTED_CONNECTS.to_string(),
        "Bytes_received" => STUB_BYTES_RECEIVED.to_string(),
        "Bytes_sent" => STUB_BYTES_SENT.to_string(),
        "Connections" => STUB_CONNECTIONS.to_string(),
        "Open_tables" => STUB_OPEN_TABLES.to_string(),
        "Questions" => live.questions.clone(),
        "Slow_queries" => STUB_SLOW_QUERIES.to_string(),
        "Threads_connected" => live.threads_connected.clone(),
        "Threads_running" => STUB_THREADS_RUNNING.to_string(),
        "Uptime" => live.uptime.clone(),
        _ => "0".to_string(),
    }
}

fn like_ci(name: &str, pattern: &str) -> bool {
    like_bytes(
        name.to_ascii_lowercase().as_bytes(),
        pattern.to_ascii_lowercase().as_bytes(),
    )
}

fn like_bytes(name: &[u8], pattern: &[u8]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some((b'%', rest)) => {
            like_bytes(name, rest) || (!name.is_empty() && like_bytes(&name[1..], pattern))
        }
        Some((b'_', rest)) => !name.is_empty() && like_bytes(&name[1..], rest),
        Some((ch, rest)) => name.first() == Some(ch) && like_bytes(&name[1..], rest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::ConnectionRegistry;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn status_map(session: &Session) -> std::collections::HashMap<String, String> {
        match show_status(None, session) {
            QueryResult::Rows { rows, .. } => rows
                .into_iter()
                .map(|r| (r[0].clone(), r[1].clone()))
                .collect(),
            other => panic!("expected SHOW STATUS rows, got {other:?}"),
        }
    }

    #[test]
    fn show_status_catalog_and_like() {
        let session = Session::new(1, "root");
        match show_status(None, &session) {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec!["Variable_name".to_string(), "Value".to_string()]
                );
                assert_eq!(rows.len(), STUB_NAMES.len());
                assert!(rows.windows(2).all(|w| w[0][0] <= w[1][0]));
                for name in STUB_NAMES {
                    assert!(rows.iter().any(|r| r[0] == *name));
                }
                let threads = rows.iter().find(|r| r[0] == "Threads_connected").unwrap();
                assert_eq!(threads[1], STUB_THREADS_CONNECTED_DEFAULT);
                let uptime = rows.iter().find(|r| r[0] == "Uptime").unwrap();
                assert_eq!(uptime[1], STUB_UPTIME);
                let questions = rows.iter().find(|r| r[0] == "Questions").unwrap();
                assert_eq!(questions[1], STUB_QUESTIONS);
            }
            other => panic!("expected SHOW STATUS rows, got {other:?}"),
        }

        let like = ShowStatementFilter::Like("Threads%".into());
        match show_status(Some(&like), &session) {
            QueryResult::Rows { rows, .. } => {
                let names: Vec<_> = rows.iter().map(|r| r[0].as_str()).collect();
                assert_eq!(names, vec!["Threads_connected", "Threads_running"]);
            }
            other => panic!("expected LIKE rows, got {other:?}"),
        }

        let miss = ShowStatementFilter::Like("not_a_real_status%".into());
        match show_status(Some(&miss), &session) {
            QueryResult::Rows { rows, .. } => {
                assert!(rows.is_empty());
            }
            other => panic!("expected empty LIKE rows, got {other:?}"),
        }
    }

    #[test]
    fn show_status_threads_connected_follows_registry() {
        let mut session = Session::new(1, "root");
        let registry = Arc::new(ConnectionRegistry::new());
        registry.register(1, "root", "%", "rusql");
        registry.register(2, "root", "%", "rusql");
        session.process_list = Some(registry);
        match show_status(None, &session) {
            QueryResult::Rows { rows, .. } => {
                let threads = rows.iter().find(|r| r[0] == "Threads_connected").unwrap();
                assert_eq!(threads[1], "2");
            }
            other => panic!("expected SHOW STATUS rows, got {other:?}"),
        }
    }

    /// M190: live Questions / Uptime from the connection registry (mock clock).
    #[test]
    fn show_status_live_questions_and_uptime() {
        let mut session = Session::new(1, "root");
        let registry = Arc::new(ConnectionRegistry::with_started_at(
            Instant::now() - Duration::from_secs(7),
        ));
        registry.register(1, "root", "%", "rusql");
        registry.increment_questions();
        registry.increment_questions();
        registry.increment_questions();
        session.process_list = Some(registry.clone());

        let map = status_map(&session);
        assert_eq!(map.get("Questions").map(String::as_str), Some("3"));
        let uptime: u64 = map
            .get("Uptime")
            .expect("Uptime")
            .parse()
            .expect("Uptime numeric");
        assert!(
            uptime >= 7,
            "Uptime from mock clock should be >= 7, got {uptime}"
        );
        assert_eq!(map.get("Threads_connected").map(String::as_str), Some("1"));
        assert_eq!(map.get("Slow_queries").map(String::as_str), Some("0"));
        assert_eq!(map.get("Bytes_sent").map(String::as_str), Some("0"));
        assert_eq!(map.get("Threads_running").map(String::as_str), Some("1"));

        let like = ShowStatementFilter::Like("Questions".into());
        match show_status(Some(&like), &session) {
            QueryResult::Rows { columns, rows } => {
                assert_eq!(
                    columns,
                    vec!["Variable_name".to_string(), "Value".to_string()]
                );
                assert_eq!(rows, vec![vec!["Questions".to_string(), "3".to_string()]]);
            }
            other => panic!("expected Questions row, got {other:?}"),
        }

        let q1 = registry.questions();
        registry.increment_questions();
        let map2 = status_map(&session);
        let q2: u64 = map2
            .get("Questions")
            .expect("Questions")
            .parse()
            .expect("Questions numeric");
        assert!(
            q2 > q1,
            "second Questions ({q2}) must be >= first ({q1}) + 1"
        );
    }
}

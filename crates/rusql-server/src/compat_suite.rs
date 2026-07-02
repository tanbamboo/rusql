//! MySQL compatibility fixture runner (JSON-driven wire tests).

use crate::wire_fixtures::{run_suite, SuiteFile};

#[tokio::test]
async fn run_basic_compat_fixtures() {
    let file: SuiteFile =
        serde_json::from_str(include_str!("../compat/basic.json")).expect("parse compat fixtures");
    for suite in &file.suites {
        run_suite(suite).await;
    }
}

#[tokio::test]
async fn compat_persistence_after_restart() {
    use crate::test_support::TestServer;
    use rusql_protocol::client_decode::QueryResponse;
    use rusql_storage::StorageEngine;

    let server = TestServer::start("persist").await;
    let mut client = server.connect().await;

    for sql in [
        "CREATE TABLE kv (k INT, v VARCHAR(8))",
        "INSERT INTO kv VALUES (1, 'one')",
        "CREATE INDEX idx_kv_k ON kv (k)",
    ] {
        match client.query(sql).await {
            QueryResponse::Ok { .. } => {}
            other => panic!("unexpected response for {sql}: {other:?}"),
        }
    }
    client.quit().await;

    let eng = server.reopen_engine();
    assert_eq!(
        eng.scan_eq("kv", "k", "1").unwrap(),
        Some(vec![vec!["1".to_string(), "one".to_string()]])
    );

    let _ = std::fs::remove_dir_all(&server.data_dir);
}

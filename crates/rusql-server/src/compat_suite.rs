//! MySQL compatibility fixture runner (JSON-driven wire tests).

use crate::test_support::TestServer;
use rusql_protocol::client_decode::QueryResponse;
use rusql_storage::StorageEngine;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct SuiteFile {
    suites: Vec<Suite>,
}

#[derive(Debug, Deserialize)]
struct Suite {
    name: String,
    steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
struct Step {
    sql: String,
    expect: Expect,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Expect {
    Ok {
        #[serde(default)]
        affected_rows: Option<u64>,
    },
    Err {
        #[serde(default)]
        code: Option<u16>,
    },
    Rows {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

fn assert_expect(got: QueryResponse, expect: &Expect, sql: &str) {
    match (got, expect) {
        (QueryResponse::Ok { affected_rows }, Expect::Ok { affected_rows: exp }) => {
            if let Some(n) = exp {
                assert_eq!(affected_rows, *n, "affected_rows mismatch for: {sql}");
            }
        }
        (QueryResponse::Err { code, message }, Expect::Err { code: exp }) => {
            if let Some(c) = exp {
                assert_eq!(code, *c, "ERR code mismatch for: {sql}");
            }
            assert!(!message.is_empty());
        }
        (
            QueryResponse::Rows { columns, rows },
            Expect::Rows {
                columns: exp_cols,
                rows: exp_rows,
            },
        ) => {
            assert_eq!(&columns, exp_cols, "columns mismatch for: {sql}");
            assert_eq!(&rows, exp_rows, "rows mismatch for: {sql}");
        }
        (got, expect) => panic!("response {got:?} did not match expect {expect:?} for: {sql}"),
    }
}

async fn run_suite(suite: &Suite) {
    let server = TestServer::start(&suite.name).await;
    let mut client = server.connect().await;
    for step in &suite.steps {
        let got = client.query(&step.sql).await;
        assert_expect(got, &step.expect, &step.sql);
    }
    client.quit().await;
    let _ = std::fs::remove_dir_all(&server.data_dir);
}

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

//! Shared JSON wire-fixture runner for compat and mysql-test subset.

use crate::test_support::TestServer;
use rusql_protocol::client_decode::QueryResponse;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SuiteFile {
    pub suites: Vec<Suite>,
}

#[derive(Debug, Deserialize)]
pub struct Suite {
    pub name: String,
    #[serde(default)]
    pub origin: Option<String>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
pub struct Step {
    pub sql: String,
    #[serde(default)]
    pub user: Option<String>,
    pub expect: Expect,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Expect {
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

pub fn assert_expect(got: QueryResponse, expect: &Expect, sql: &str) {
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

pub async fn run_suite(suite: &Suite) {
    let _origin = suite.origin.as_deref().unwrap_or("unknown");
    let server = TestServer::start(&suite.name).await;
    let mut client = server.connect().await;
    let mut current_user = "root".to_string();
    for step in &suite.steps {
        let user = step.user.as_deref().unwrap_or("root");
        if user != current_user {
            client.quit().await;
            client = server.connect_as(user, "").await;
            current_user = user.to_string();
        }
        let got = client.query(&step.sql).await;
        assert_expect(got, &step.expect, &step.sql);
    }
    client.quit().await;
    let _ = std::fs::remove_dir_all(&server.data_dir);
}

//! Oracle mysql-test inspired subset (wire harness via internal test client).

use crate::wire_fixtures::{run_suite, SuiteFile};
use std::path::PathBuf;

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/mysql-test/manifest.json")
}

#[tokio::test]
async fn run_mysql_test_subset() {
    let text =
        std::fs::read_to_string(manifest_path()).expect("read tests/mysql-test/manifest.json");
    let file: SuiteFile = serde_json::from_str(&text).expect("parse mysql-test manifest");
    assert!(
        file.suites.len() >= 10,
        "mysql-test subset requires 10+ cases, got {}",
        file.suites.len()
    );
    for suite in &file.suites {
        run_suite(suite).await;
    }
}

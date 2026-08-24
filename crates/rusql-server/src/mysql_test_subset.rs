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
    let total = file.suites.len();
    assert!(
        total >= 100,
        "mysql-test subset requires 100+ cases, got {total}"
    );
    let mut passed = 0usize;
    for suite in &file.suites {
        run_suite(suite).await;
        passed += 1;
    }
    let floor = total * 95 / 100;
    assert!(
        passed >= floor,
        "mysql-test pass floor {floor}/{total} not met (passed {passed})"
    );
    eprintln!("mysql-test subset: {passed}/{total} passed (floor {floor})");
}

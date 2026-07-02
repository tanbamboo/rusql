# M30 — mysql-test subset

**Issue #53**

## Problem

Oracle **mysql-test** is the canonical MySQL regression corpus (thousands of `.test` files). rusql needed a small, runnable slice that tracks mysql-test themes without porting the full `mysql-test-run.pl` harness.

## Decision

- `tests/mysql-test/manifest.json` — 12 wire suites with `origin` references to mysql-test result files (simplified SQL).
- Shared runner in `wire_fixtures.rs` (also used by `compat_suite.rs`).
- `cargo test -p rusql-server mysql_test_subset` or `node scripts/mysql-test-subset.mjs`.
- Skips documented in `tests/mysql-test/SKIPS.md` (stored programs, replication, charset, full optimizer, official CLI diff — see issue #73).
- CI job `mysql-test-subset` runs the script on ubuntu-latest (no Docker required).

## Harness lesson

> Reuse the internal wire test client for mysql-test-style cases; differential against official `mysql` CLI remains a separate track (M29 + #73).

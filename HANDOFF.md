# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-01 |
| Branch | feat/m59-m61 |
| Next step | Merge PR; label next `agent-ready` issue from roadmap |

## Recent Progress

- **M59** — `rusql-core::collation` module (`utf8mb4_unicode_ci`); `ORDER BY` / `WHERE =` / `IN` / `BETWEEN` / `LIKE` equality; `SHOW COLLATION`; corpus tests (≥12 equal pairs, 11-string sort order).
- **M61** — `scripts/sysbench-rusql.mjs`, `.github/workflows/sysbench.yml`, sbtest DDL docs in user-guide (en/zh-CN).
- Sensors green: `cargo fmt`, `clippy`, `test`, `harness-validate`.

## Verification

```bash
cargo test -p rusql-core collation
cargo test -p rusql-executor collation_order_by
cargo test -p rusql-executor show_collation
node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308  # optional; soft-fail without Docker
```

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
```

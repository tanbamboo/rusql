# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-08-19 |
| Branch | main |
| Next step | Next QA parity milestone after M36–M38 / M45 — poll `agent-ready` issues |

## Recent Progress

- **#132** merged: M45 extended WHERE (OR/NOT/LIKE/BETWEEN/IN)
- **#133** merged: M36 CREATE/DROP DATABASE + multi-schema catalog
- **#134** merged: M37 AUTO_INCREMENT + WAL counter
- **#135** merged: M38 ALTER TABLE DROP/MODIFY/RENAME COLUMN + RENAME TABLE
- CI: official `mysql`/`mysqladmin` oracle tests hang on `ubuntu-latest` rust job (client present on PATH). Gated behind `RUSQL_ORACLE_MYSQL=1` on CI; coverage remains in mysql-diff/smoke jobs.

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/check-book.mjs
```

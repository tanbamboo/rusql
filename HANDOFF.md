# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-08-10 |
| Branch | main |
| Next step | Roadmap beyond M35 — harness hardening or new compat gaps |

## Recent Progress

- **#28** book complete through M35 (en + zh-CN chapters)
- **#87** merged: COM_PING / mysqladmin ping (PR #88)
- **#58** merged: utf8mb4 charset metadata (PR #84)
- **#56** merged: SQL views (PR #85)
- **#57** merged: binlog QUERY_EVENT spike (PR #86)
- **#55** merged: MVCC snapshot isolation (PR #83)
- **#77** merged: COM_INIT_DB / USE rusql (PR #82)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/check-book.mjs
```

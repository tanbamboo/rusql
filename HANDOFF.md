# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-14 |
| Branch | feat/m76-connection-id-row-count |
| Next step | After M76 merge: file [M77](docs/en/specs/mysql-full-parity-roadmap.md) (`FOUND_ROWS()` / `@@` session variables) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-14)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #184); M76 shipping |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M76 | M62–M75 merged (#162–#184); M76 CONNECTION_ID / ROW_COUNT in this PR |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `FOUND_ROWS()` / more `@@` session variables — next after M76
2. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **M76 (this PR)** — `CONNECTION_ID()` + `ROW_COUNT()` session functions (#185)
- **#184 merged** — M75 `LAST_INSERT_ID()` + INSERT OK `last_insert_id` (#183)
- **#182 merged** — M74 `UPDATE_ROWS` / `DELETE_ROWS` (#181)
- **#180 merged** — M73 live `COM_BINLOG_DUMP` follow (#179)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-14 |
| Branch | feat/m75-last-insert-id |
| Next step | Merge M75 LAST_INSERT_ID (#183); then file the next Phase Q ORM gap |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-14)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #182) pending M75 |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M75 | M62–M74 merged (#162–#182); M75 LAST_INSERT_ID in this PR |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `CONNECTION_ID()` / `ROW_COUNT()` session functions (next after M75 merge)
2. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **M75** — `LAST_INSERT_ID()` + INSERT OK `last_insert_id` (#183)
- **#182 merged** — M74 `UPDATE_ROWS` / `DELETE_ROWS` (#181)
- **#180 merged** — M73 live `COM_BINLOG_DUMP` follow (#179)
- **#178 merged** — M72 `TABLE_MAP` + `WRITE_ROWS` for INSERT (#177)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

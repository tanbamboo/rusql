# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-14 |
| Branch | main |
| Next step | Implement [M77 @@ session variables](https://github.com/tanbamboo/rusql/issues/187) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-14)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #186) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M76 | Merged (#162–#186) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `@@` session/system variables for client probes — [M77 #187](https://github.com/tanbamboo/rusql/issues/187)
2. `FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS` (deprecated in MySQL 8.0.17; lower priority)
3. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#186 merged** — M76 `CONNECTION_ID()` / `ROW_COUNT()` (#185)
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

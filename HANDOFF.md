# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-15 |
| Branch | main |
| Next step | Implement [M78 FOUND_ROWS() / SQL_CALC_FOUND_ROWS](https://github.com/tanbamboo/rusql/issues/189) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-15)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #188) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M77 | Merged (#162–#188) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS` (deprecated in MySQL 8.0.17) — [M78 #189](https://github.com/tanbamboo/rusql/issues/189)
2. Broader `@@` stubs (e.g. `auto_increment_increment`, `time_zone`, isolation) and `SHOW VARIABLES`
3. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#188 merged** — M77 `@@` session/system variable stubs (#187)
- **#186 merged** — M76 `CONNECTION_ID()` / `ROW_COUNT()` (#185)
- **#184 merged** — M75 `LAST_INSERT_ID()` + INSERT OK `last_insert_id` (#183)
- **#182 merged** — M74 `UPDATE_ROWS` / `DELETE_ROWS` (#181)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

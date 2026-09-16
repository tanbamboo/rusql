# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-16 |
| Branch | main |
| Next step | Implement [M81 SET @@](https://github.com/tanbamboo/rusql/issues/195) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-16)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #194) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M80 | Merged (#162–#194) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `SET @@` / session variable persistence — [M81 #195](https://github.com/tanbamboo/rusql/issues/195)
2. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#194 merged** — M80 `SHOW VARIABLES` stub catalog (#193): session/global lists and `LIKE` over the M77+M79 `@@` set
- **#192 merged** — M79 more `@@` connector probes (#191): `auto_increment_increment`, `time_zone`/`system_time_zone`, `transaction_isolation`/`tx_isolation`, `max_allowed_packet`, `license`
- **#190 merged** — M78 `FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS` (#189)
- **#188 merged** — M77 `@@` session/system variable stubs (#187)
- **#186 merged** — M76 `CONNECTION_ID()` / `ROW_COUNT()` (#185)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-15 |
| Branch | main |
| Next step | Implement [M79 more @@ connector probes](https://github.com/tanbamboo/rusql/issues/191) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-15)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #190) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M78 | Merged (#162–#190) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. Broader `@@` stubs for JDBC/ORM handshake (`auto_increment_increment`, `time_zone`, isolation) — [M79 #191](https://github.com/tanbamboo/rusql/issues/191)
2. `SHOW VARIABLES` catalog
3. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#190 merged** — M78 `FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS` (#189)
- **#188 merged** — M77 `@@` session/system variable stubs (#187)
- **#186 merged** — M76 `CONNECTION_ID()` / `ROW_COUNT()` (#185)
- **#184 merged** — M75 `LAST_INSERT_ID()` + INSERT OK `last_insert_id` (#183)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

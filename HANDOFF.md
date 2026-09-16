# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-16 |
| Branch | feat/m80-show-variables |
| Next step | Merge M80 (#193) then file `SET @@` / session variable persistence |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-16)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #192); M80 in flight |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M79 | Merged (#162–#192) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `SHOW VARIABLES` catalog over the documented @@ stub set — [M80 #193](https://github.com/tanbamboo/rusql/issues/193) (this PR)
2. `SET @@` / session variable persistence
3. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **M80 in flight** — `SHOW VARIABLES` stub catalog over M77+M79 names (#193)
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

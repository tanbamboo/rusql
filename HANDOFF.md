# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-14 |
| Branch | main |
| Next step | Implement [M74 UPDATE/DELETE row events](https://github.com/tanbamboo/rusql/issues/181) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-14)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #180) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M73 | Merged (#162–#180) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. UPDATE/DELETE row events — [M74 #181](https://github.com/tanbamboo/rusql/issues/181)
2. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#180 merged** — M73 live `COM_BINLOG_DUMP` follow (#179)
- **#178 merged** — M72 `TABLE_MAP` + `WRITE_ROWS` for INSERT (#177)
- **#176 merged** — M71 per-event `COM_BINLOG_DUMP` (#175)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-14 |
| Branch | feat/m73-binlog-dump-follow |
| Next step | Merge M73 (#179), then file M74 UPDATE/DELETE row events |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-14)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #178) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M72 | Merged (#162–#178) |
| M73 | This branch — live `COM_BINLOG_DUMP` follow (#179) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. UPDATE/DELETE row events (next after M73 merge)
2. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **M73 (this branch)** — live dump follow: flags `0` streams later COMMITs; `BINLOG_DUMP_NON_BLOCK` stays one-shot (#179)
- **#178 merged** — M72 `TABLE_MAP` + `WRITE_ROWS` for INSERT (#177)
- **#176 merged** — M71 per-event `COM_BINLOG_DUMP` (#175)
- **#174 merged** — M70 window ranking `ROW_NUMBER` / `RANK` / `DENSE_RANK` (#173)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

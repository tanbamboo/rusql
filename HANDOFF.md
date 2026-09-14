# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-14 |
| Branch | main |
| Next step | Implement [M73 live dump follow](https://github.com/tanbamboo/rusql/issues/179) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-14)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #178) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M72 | Merged (#162–#178) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. Live `COM_BINLOG_DUMP` follow — [M73 #179](https://github.com/tanbamboo/rusql/issues/179)
2. UPDATE/DELETE row events

## Recent Progress

- **#178 merged** — M72 `TABLE_MAP` + `WRITE_ROWS` for INSERT (#177)
- **#176 merged** — M71 per-event `COM_BINLOG_DUMP` (#175)
- **#174 merged** — M70 window ranking `ROW_NUMBER` / `RANK` / `DENSE_RANK` (#173)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

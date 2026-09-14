# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-14 |
| Branch | feat/m72-write-rows |
| Next step | Sensors + PR for [M72 WRITE_ROWS](https://github.com/tanbamboo/rusql/issues/177); then live dump follow |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-14)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #176); M72 in progress on this branch |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M71 | Merged (#162–#176) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. Row-based binlog for INSERT (`TABLE_MAP` / `WRITE_ROWS`) — M72 (this branch)
2. Live dump follow; UPDATE/DELETE row events

## Recent Progress

- **#176 merged** — M71 per-event `COM_BINLOG_DUMP` (#175)
- **#174 merged** — M70 window ranking `ROW_NUMBER` / `RANK` / `DENSE_RANK` (#173)
- **#172 merged** — M69 non-recursive `WITH` CTE (#171)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

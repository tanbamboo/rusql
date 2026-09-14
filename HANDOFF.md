# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-14 |
| Branch | main |
| Next step | Ship M71 (#175) then row-based binlog / live dump follow |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-14)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #174) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M70 | Merged (#162–#174) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. Deeper replication: per-event `COM_BINLOG_DUMP` (M71), then row events / live follow

## Recent Progress

- **#174 merged** — M70 window ranking `ROW_NUMBER` / `RANK` / `DENSE_RANK` (#173)
- **#172 merged** — M69 non-recursive `WITH` CTE (#171)
- **#170 merged** — M68 `INSERT … SELECT` / `ON DUPLICATE KEY UPDATE` (#169)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

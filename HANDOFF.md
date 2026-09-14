# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-14 |
| Branch | main |
| Next step | Implement [M70 window functions](https://github.com/tanbamboo/rusql/issues/173) (`ROW_NUMBER` / `RANK` / `DENSE_RANK`) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-14)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #172) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M69 | Merged (#162–#172) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. Window functions (`ROW_NUMBER` / `RANK` / `DENSE_RANK`)
2. Deeper replication beyond MVP stubs

## Recent Progress

- **#172 merged** — M69 non-recursive `WITH` CTE (#171)
- **#170 merged** — M68 `INSERT … SELECT` / `ON DUPLICATE KEY UPDATE` (#169)
- **#168 merged** — M67 SELECT DISTINCT (#167)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-14 |
| Branch | feat/m69-with-cte |
| Next step | After M69 ships: file + implement window functions |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-14)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #170) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M68 | Merged (#162–#170) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. Window functions
2. Deeper replication beyond MVP stubs

## Recent Progress

- **#171 filed + implementing** — M69 non-recursive `WITH` (CTE)
- **#170 merged** — M68 `INSERT … SELECT` / `ON DUPLICATE KEY UPDATE` (#169)
- **#168 merged** — M67 SELECT DISTINCT (#167)
- **#166 merged** — M66 CASE/IF
- **#164 merged** — M65 session info functions

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

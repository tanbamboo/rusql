# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-07 |
| Branch | feat/m67-select-distinct |
| Next step | Land PR for M67 [#167](https://github.com/tanbamboo/rusql/issues/167); then file INSERT…SELECT / ON DUPLICATE KEY |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-07)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (#160–#166) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M66 | Merged |
| M67 SELECT DISTINCT | In progress (#167) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. **M67** (in progress #167): `SELECT DISTINCT`
2. `INSERT … SELECT` / `ON DUPLICATE KEY UPDATE`
3. CTEs / window functions
4. Deeper replication beyond MVP stubs

## Recent Progress

- **#167** — M67 SELECT DISTINCT (this branch)
- **#166 merged** — M66 CASE/IF
- **#164 merged** — M65 session info functions
- **#162 merged** — M62 `utf8mb4_0900_ai_ci`

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

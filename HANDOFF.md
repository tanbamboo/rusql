# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-07 |
| Branch | feat/m66-case-if-expressions |
| Next step | Land PR [#166](https://github.com/tanbamboo/rusql/pull/166) (M66 #165) when CI green; then file M67 `SELECT DISTINCT` |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-07)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (#160, #162, #164) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M65 | Merged |
| M66 CASE/IF | In progress (#165) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. **M66** (in progress #165): `CASE` / `IF`
2. `SELECT DISTINCT`
3. `INSERT … SELECT` / `ON DUPLICATE KEY UPDATE`
4. CTEs / window functions
5. Deeper replication beyond MVP stubs

## Recent Progress

- **#165** — M66 CASE/IF (this branch)
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

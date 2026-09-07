# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-07 |
| Branch | feat/m65-session-info-functions |
| Next step | Finish M65 (#163) sensors + PR; then file/implement next Phase Q gaps (`CASE`/`DISTINCT`/`INSERT SELECT`) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-07)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (#160 CI fix, #162 M62) |
| Roadmap M36–M61 + PERF-B* | Complete (no open agent-ready backlog before #163) |
| M62 collation | Merged (#162 / #153) |
| M63–M64 | Merged earlier |
| Estimated surface | ~45–70% client-visible path; far from 100% |

## Gaps (priority order for Phase Q)

1. **M65** (in progress #163): `DATABASE`/`USER`/`VERSION` — blocks ORM/CLI introspection
2. `CASE` / `IF` expressions
3. `SELECT DISTINCT`
4. `INSERT … SELECT` / `ON DUPLICATE KEY UPDATE`
5. CTEs / window functions
6. Deeper replication beyond MVP stubs

## Recent Progress

- **#162 merged** — M62 `utf8mb4_0900_ai_ci`
- **#160 merged** — CI green (#158 fmt, #159 mysql-diff USE via `-D`)
- **#163 opened** — M65 session info functions (`agent-ready` P1)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

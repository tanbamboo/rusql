# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-18 |
| Branch | main |
| Next step | Implement [M101 SHOW EVENTS](https://github.com/tanbamboo/rusql/issues/236) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-18)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #235) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M100 | Merged (#162–#235) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `SHOW EVENTS` stubs — [M101 #236](https://github.com/tanbamboo/rusql/issues/236)
2. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#235 merged** — M100 `SHOW CREATE EVENT` (#234): statement accepted; no event catalog yet so every name is errno 1539; `SHOW CREATE USER` / `SHOW FUNCTION STATUS` / `SHOW PROCEDURE STATUS` unchanged
- **#233 merged** — M99 `SHOW CREATE USER` (#232): MySQL-shaped `CREATE USER for {user}@{host}` reconstructed from M55 accounts (`IDENTIFIED WITH '{plugin}'`, no hash/`BY`/`AS`); unknown accounts errno 3162
- **#230 merged** — M98 `SHOW FUNCTION STATUS` (#229): MySQL-shaped columns from catalog `FunctionMeta`; `Type` is `FUNCTION`; stub Definer/timestamps/charset; unmatched `LIKE` returns zero rows

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

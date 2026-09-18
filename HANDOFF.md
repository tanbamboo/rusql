# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-18 |
| Branch | main |
| Next step | Implement [M103 ALTER EVENT catalog](https://github.com/tanbamboo/rusql/issues/240) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-18)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #239) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M102 | Merged (#162–#239) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. `ALTER EVENT` catalog — [M103 #240](https://github.com/tanbamboo/rusql/issues/240)
2. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#239 merged** — M102 `CREATE EVENT` catalog (#238): persist `EventMeta`; `SHOW EVENTS` lists rows; `SHOW CREATE EVENT` reconstructs DDL; duplicate errno 1537; `DROP EVENT`; no scheduler execution
- **#237 merged** — M101 `SHOW EVENTS` (#236): MySQL-shaped columns over an empty catalog; unmatched `LIKE` is zero rows; unknown `FROM` db errno 1049; `SHOW CREATE EVENT` / `SHOW CREATE USER` / `SHOW FUNCTION STATUS` unchanged
- **#235 merged** — M100 `SHOW CREATE EVENT` (#234): statement accepted; unknown names errno 1539 until the M102 catalog exists

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

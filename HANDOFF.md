# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-19 |
| Branch | main |
| Next step | Implement [M104 event scheduler due AT](https://github.com/tanbamboo/rusql/issues/242) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-19)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #241) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M103 | Merged (#162–#241) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. Event scheduler executes due one-time `AT` events — [M104 #242](https://github.com/tanbamboo/rusql/issues/242)
2. Recurring `EVERY` ticking, last-executed timestamps, DEFINER / ON COMPLETION stay later
3. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#241 merged** — M103 `ALTER EVENT` catalog (#240): update schedule (`AT`/`EVERY`), `ENABLE`/`DISABLE`, `RENAME TO`, `DO`; unknown errno 1539; `SHOW EVENTS` / `SHOW CREATE EVENT` reflect the row; no scheduler execution
- **#239 merged** — M102 `CREATE EVENT` catalog (#238): persist `EventMeta`; `SHOW EVENTS` lists rows; `SHOW CREATE EVENT` reconstructs DDL; duplicate errno 1537; `DROP EVENT`; no scheduler execution
- **#237 merged** — M101 `SHOW EVENTS` (#236): MySQL-shaped columns over an empty catalog; unmatched `LIKE` is zero rows; unknown `FROM` db errno 1049

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

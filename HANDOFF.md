# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-19 |
| Branch | main |
| Next step | Implement [M105 event scheduler EVERY](https://github.com/tanbamboo/rusql/issues/244) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-19)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #243) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M104 | Merged (#162–#243) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. Event scheduler executes recurring `EVERY` intervals — [M105 #244](https://github.com/tanbamboo/rusql/issues/244)
2. `STARTS` / `ENDS`, last-executed on `SHOW EVENTS`, DEFINER / ON COMPLETION stay later
3. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#243 merged** — M104 event scheduler due `AT` (#242): ENABLED one-time `AT` events run `DO` on the next COM_QUERY then drop; `@@event_scheduler` is a read-only `ON` stub; `EVERY` / future `AT` / `DISABLED` are not run
- **#241 merged** — M103 `ALTER EVENT` catalog (#240): update schedule (`AT`/`EVERY`), `ENABLE`/`DISABLE`, `RENAME TO`, `DO`; unknown errno 1539
- **#239 merged** — M102 `CREATE EVENT` catalog (#238): persist `EventMeta`; `SHOW EVENTS` lists rows; `SHOW CREATE EVENT` reconstructs DDL; `DROP EVENT`

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-19 |
| Branch | main |
| Next step | Implement [M106 event STARTS/ENDS](https://github.com/tanbamboo/rusql/issues/246) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-19)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #245) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M105 | Merged (#162–#245) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. Event scheduler `STARTS` / `ENDS` — [M106 #246](https://github.com/tanbamboo/rusql/issues/246)
2. DEFINER / ON COMPLETION / last-executed on `SHOW EVENTS` stay later
3. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#245 merged** — M105 event scheduler `EVERY` (#244): first fire on next COM_QUERY, then `last_executed + interval`; catalog row stays; `MONTH`/`YEAR` 30/365-day approximations
- **#243 merged** — M104 due `AT` (#242): one-time events run `DO` then drop; `@@event_scheduler` read-only `ON`
- **#241 merged** — M103 `ALTER EVENT` catalog (#240)

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

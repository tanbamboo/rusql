# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-20 |
| Branch | main |
| Next step | Implement [M107 event DEFINER / ON COMPLETION](https://github.com/tanbamboo/rusql/issues/248) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-20)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #247) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M106 | Merged (#162–#247) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. Event `DEFINER` / `ON COMPLETION` — [M107 #248](https://github.com/tanbamboo/rusql/issues/248)
2. COMMENT / last-executed on `SHOW EVENTS` stay later
3. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **#247 merged** — M106 event `STARTS`/`ENDS` (#246): `EVERY` gated by inclusive window; `SHOW EVENTS` Starts/Ends and `SHOW CREATE EVENT` reconstruct them
- **#245 merged** — M105 event scheduler `EVERY` (#244): first fire on next COM_QUERY, then `last_executed + interval`; catalog row stays; `MONTH`/`YEAR` 30/365-day approximations
- **#243 merged** — M104 due `AT` (#242): one-time events run `DO` then drop; `@@event_scheduler` read-only `ON`

## Sensors

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --skip release_binary
node scripts/harness-validate.mjs
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs   # requires Docker
```

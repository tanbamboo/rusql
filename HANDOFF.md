# HANDOFF — Cross-Session State

| Field | Value |
|-------|-------|
| Last updated | 2026-09-20 |
| Branch | feat/m107-event-definer-on-completion |
| Next step | Merge M107 (#248), then file M108 event COMMENT / `information_schema.EVENTS` (not a 16th `SHOW EVENTS` column) |

## Ultimate goal

**MySQL 8.0 functional equivalence** (wire, SQL, metadata, security, replication) — [mysql-full-parity-roadmap.md](docs/en/specs/mysql-full-parity-roadmap.md).

## Status vs goal (2026-09-20)

| Layer | Status |
|-------|--------|
| CI on `main` | Green (PR #247) |
| Roadmap M36–M61 + PERF-B* | Complete |
| M62–M107 | M107 implemented on this branch (Closes #248); M62–M106 merged (#162–#247) |
| Estimated surface | ~45–70% client-visible; growing via Phase Q |

## Gaps (priority order for Phase Q)

1. Event `COMMENT` persist + `SHOW CREATE EVENT` reconstruction (M108, not yet filed)
2. `information_schema.EVENTS` (`EVENT_COMMENT`, `LAST_EXECUTED`, `DEFINER`, `ON_COMPLETION`) — MySQL has no last-executed column on `SHOW EVENTS` (15 columns)
3. Further replication (GTID event 33, heartbeat) stays out of scope until later slices

## Recent Progress

- **M107 on branch** — Event `DEFINER` / `ON COMPLETION` persist; `SHOW EVENTS` Definer and `SHOW CREATE EVENT` reconstruct them; `PRESERVE` keeps due `AT` as `DISABLED` (#248)
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

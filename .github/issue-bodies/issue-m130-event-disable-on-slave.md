## Goal

Accept `CREATE EVENT … DISABLE ON SLAVE` (and `ENABLE ON SLAVE` if cheap) so replica event-control DDL is not a parse error.

## Background

M102–M108 event catalog exists. Probe: `event_disable_on_slave`. Persist a flag; scheduler skips events that are slave-disabled **on this process** if we have no replica role yet — document: treat as DISABLED for execution, still listable in SHOW EVENTS.

## Acceptance Criteria

- [ ] `CREATE EVENT e_dos ON SCHEDULE EVERY 1 HOUR DISABLE ON SLAVE DO SELECT 1` persists
- [ ] `SHOW CREATE EVENT e_dos` reconstructs `DISABLE ON SLAVE` (or documented equivalent)
- [ ] Scheduler does not run the event while the flag is set (same as DISABLED)
- [ ] Existing ENABLE/DISABLE without ON SLAVE unchanged
- [ ] Unit/wire tests; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**` (stored program / event parse)
- `crates/rusql-core/src/programs.rs` (`serde(default)` flag)
- `crates/rusql-executor/src/**`
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing `SHOW EVENTS` column count (stays 15)
- GTID / replica applier behavior beyond skip-execute
- WAL JSON for tables

## Negative Constraints

- Do not implement replica `server_id` gating unless tests exist
- Additive serde default on EventMeta only

## Test plan

```bash
cargo test -p rusql-executor disable_on_slave
cargo test -p rusql-server disable_on_slave
```

## Goal

Implement `SHOW CREATE EVENT` so clients that probe event DDL get a MySQL-shaped response (or MySQL-like missing-event errno) instead of an unsupported-statement / parse error.

## Background

Phase Q after M99. rusql has no event scheduler or `CREATE EVENT` catalog yet. sqlparser 0.53 may not expose `SHOW CREATE EVENT`; it typically needs a parse rewrite (same pattern as `SHOW CREATE PROCEDURE`). phpMyAdmin and dump tools still send `SHOW CREATE EVENT name`. This slice accepts the statement and returns a MySQL-like missing-event error until a later `CREATE EVENT` catalog exists. Reconstructed success-path DDL, `SHOW EVENTS`, per-database charset clauses, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW CREATE EVENT name` (and `db.name` if a one-line parse is natural) is accepted (not sqlparser 1064)
- [ ] Unknown / any event is an error (MySQL-like errno 1539 `ER_EVENT_DOES_NOT_EXIST` if a one-line `ExecError::Mysql` path is natural)
- [ ] `SHOW CREATE USER` from M99, `SHOW FUNCTION STATUS` from M98, and `SHOW PROCEDURE STATUS` from M97 are unchanged
- [ ] Unit/wire tests; `mysql-diff` may use a missing-event probe (both sides error is OK); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW CREATE EVENT` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- `CREATE EVENT` / event scheduler persistence
- `SHOW EVENTS`
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW CREATE EVENT` parity; document missing-event errno until a catalog exists
- Do not change `SHOW CREATE USER` output from M99
- Do not implement `CREATE EVENT` or `SHOW EVENTS`
- Do not invent DEFINER / ON COMPLETION / event-body reconstruction

## Test plan

```bash
cargo test -p rusql-sql show_create_event
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server show_create_event
```

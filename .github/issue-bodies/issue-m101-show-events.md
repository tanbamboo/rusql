## Goal

Implement `SHOW EVENTS` so clients that list scheduled events get MySQL-shaped columns (currently zero rows) instead of an unsupported-statement / parse error.

## Background

Phase Q after M100. rusql has no event scheduler or `CREATE EVENT` catalog yet. sqlparser 0.53 may not expose `SHOW EVENTS`; it typically needs a parse rewrite (same pattern as `SHOW FUNCTION STATUS`). phpMyAdmin and dump tools send `SHOW EVENTS` (optional `FROM`/`IN` db and `LIKE`). This slice accepts the statement and returns MySQL-shaped columns over an empty catalog until a later `CREATE EVENT` catalog exists. `CREATE EVENT` persistence, reconstructed `SHOW CREATE EVENT` success-path DDL, per-database charset clauses, and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW EVENTS` returns MySQL-shaped columns including at least `Db`, `Name`, `Status` (add `Definer`, `Time zone`, `Type`, `Execute at`, `Interval value`, `Interval field`, `Starts`, `Ends`, `Originator`, `character_set_client`, `collation_connection`, `Database Collation` if a one-line MySQL-shaped row is natural)
- [ ] With no event catalog, the result is zero rows (not a parse error)
- [ ] `SHOW EVENTS LIKE 'pattern'` is accepted; unmatched `LIKE` returns zero rows
- [ ] Optional `FROM`/`IN` db if a one-line parse is natural; unknown db may use existing missing-database handling (errno 1049)
- [ ] `SHOW CREATE EVENT` from M100, `SHOW CREATE USER` from M99, and `SHOW FUNCTION STATUS` from M98 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (empty stub vs Docker MySQL event list); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW EVENTS` if needed)
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
- Reconstructing success-path `SHOW CREATE EVENT` DDL
- Generating live truncation / strict-mode warnings
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW EVENTS` parity; document an empty catalog until `CREATE EVENT` exists
- Do not change `SHOW CREATE EVENT` errno 1539 from M100
- Do not implement `CREATE EVENT`
- Do not invent live Definer / last-executed timestamps

## Test plan

```bash
cargo test -p rusql-sql show_events
cargo test -p rusql-executor show_events
cargo test -p rusql-server show_events
```

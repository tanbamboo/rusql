## Goal

Implement `SHOW WARNINGS` / `SHOW ERRORS` so clients and drivers that probe the session diagnostic list get MySQL-shaped columns instead of an unsupported-statement error.

## Background

Phase Q after M89. sqlparser 0.53 has no `Statement::ShowWarnings`; `SHOW WARNINGS` typically lands as `ShowVariable` or a parse rewrite (same pattern as `SHOW ENGINES` / `SHOW CHARACTER SET`). Official CLI, JDBC, and phpMyAdmin send `SHOW WARNINGS` (and often `SHOW ERRORS`) after DML/DDL. This slice is a documented stub: the list is empty unless a one-line existing diagnostic is already available. Generating truncation/strict-mode warnings and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW WARNINGS` returns MySQL-shaped columns `Level`, `Code`, `Message`
- [ ] After a successful statement with no diagnostics, the result is zero rows (not an unsupported-statement error)
- [ ] `SHOW ERRORS` is equivalent to `SHOW WARNINGS` for this slice (or a documented empty subset)
- [ ] `SHOW CHARACTER SET` stubs from M89 and `SHOW ENGINES` stubs from M88 are unchanged
- [ ] Unit/wire tests; `mysql-diff` (empty list after `SELECT 1` can compare; use `compare_output: false` if charset/CLI noise appears); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW WARNINGS` / `SHOW ERRORS` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- Generating live truncation / strict-mode / note diagnostics
- `GET DIAGNOSTICS` / `SHOW COUNT(*) WARNINGS` unless a one-line existing counter is already available
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 warning/note parity; document the empty (or tiny) stub
- Do not implement warning generation on INSERT truncation or sql_mode
- Do not implement `SHOW WARNINGS LIMIT n` unless sqlparser already exposes it as a one-line wire-up
- Do not implement `WHERE` filtering unless sqlparser already exposes it as a one-line wire-up

## Test plan

```bash
cargo test -p rusql-sql warnings
cargo test -p rusql-executor warnings
cargo test -p rusql-server warnings
```

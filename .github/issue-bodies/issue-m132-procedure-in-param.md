## Goal

Parse and execute `CREATE PROCEDURE name (IN x INT) …` / `CALL name(literal)` so the IN-parameter probe is not a dialect failure.

## Background

Probe `procedure_in_param` currently both-fail (DELIMITER / sqlparser). P3 MVP procedures have empty param lists. This slice: one `IN` parameter, referenced in `SELECT x` inside BEGIN…END. Official CLI `DELIMITER` remains M172 if still required for multi-stmt; wire `COM_QUERY` with the full CREATE text must work.

## Acceptance Criteria

- [ ] `CREATE PROCEDURE gap_p(IN x INT) BEGIN SELECT x; END` succeeds via rusql wire (single COM_QUERY)
- [ ] `CALL gap_p(3)` returns a one-column result `3`
- [ ] `SHOW CREATE PROCEDURE gap_p` includes `IN x INT` (no longer forced empty list)
- [ ] Existing zero-arg procedures still work
- [ ] Unit/wire tests; `mysql-diff` if Docker accepts the same CREATE without DELIMITER; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-sql/src/stored_programs.rs` and parse
- `crates/rusql-core/src/programs.rs` (param metadata, serde default)
- `crates/rusql-executor/src/programs.rs` and related
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- `OUT`/`INOUT` (M165)
- `SIGNAL` (M166)
- Changing DELIMITER server-wide (M172)
- WAL table format

## Negative Constraints

- Do not invent parameters for old catalog rows (`serde(default)` empty vec)
- Do not require the mysql CLI DELIMITER for the wire test

## Test plan

```bash
cargo test -p rusql-sql procedure_in
cargo test -p rusql-executor procedure_in
cargo test -p rusql-server procedure_in
```

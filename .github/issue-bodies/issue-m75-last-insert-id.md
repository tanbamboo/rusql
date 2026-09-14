## Goal

Expose MySQL `LAST_INSERT_ID()` (and OK-packet `last_insert_id`) so clients and ORMs can read the auto-increment value from the last `INSERT` on the connection.

## Background

Phase Q after M74. `AUTO_INCREMENT` (M37) already assigns ids. The OK packet has a `last_insert_id` field but it is always 0, and `SELECT LAST_INSERT_ID()` is unsupported. This slice wires session state from INSERT into both the function and the OK packet.

## Acceptance Criteria

- [ ] `INSERT` into an `AUTO_INCREMENT` column updates connection `last_insert_id` to the generated id
- [ ] The OK packet for that INSERT reports the same value in `last_insert_id`
- [ ] `SELECT LAST_INSERT_ID()` returns that value on the same connection (and 0 if no INSERT yet)
- [ ] A second connection does not see the first connection’s id
- [ ] Explicit `INSERT … VALUES (n)` into an auto-inc column sets `LAST_INSERT_ID()` to `n` when MySQL would (document if rusql only tracks generated ids)
- [ ] Unit/wire tests; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (function parse if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-core/src/**` (session field if needed)
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout (M72–M74)
- `INSERT … SET last_insert_id = …` session user variable beyond the function
- New crates

## Negative Constraints

- Do not implement `LAST_INSERT_ID(expr)` setter form in this issue unless it is a one-line follow-on with tests
- Do not change AUTO_INCREMENT allocation semantics from M37

## Test plan

```bash
cargo test -p rusql-executor last_insert
cargo test -p rusql-server last_insert
```

## Goal

Accept `SAVEPOINT`, `ROLLBACK TO SAVEPOINT`, and `RELEASE SAVEPOINT` so nested rollback is not unsupported.

## Background

M9 `BEGIN`/`COMMIT`/`ROLLBACK` exist (transaction overlay). Probe: `savepoint`. Savepoints snapshot the overlay; `ROLLBACK TO` restores it; `RELEASE` drops the name.

## Acceptance Criteria

- [ ] Inside a transaction: `SAVEPOINT gap_sp1` succeeds
- [ ] After further INSERT, `ROLLBACK TO SAVEPOINT gap_sp1` undoes those inserts but keeps the transaction open; the savepoint row from before remains
- [ ] `RELEASE SAVEPOINT gap_sp1` then `ROLLBACK TO SAVEPOINT gap_sp1` errors (MySQL 1305 / documented)
- [ ] `SAVEPOINT` outside a transaction: match MySQL (implicit start or error — pin tests)
- [ ] Unit/wire tests; `mysql-diff`; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF, rusql-vs-mysql

## File Boundaries

Allowed:
- `crates/rusql-sql/src/**` if sqlparser savepoint needs rewrite
- `crates/rusql-executor/src/**`
- `crates/rusql-storage/src/txn.rs` (overlay snapshots)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Gap locks / FOR UPDATE wait
- Nested transactions (`START TRANSACTION` inside txn) beyond savepoints

## Negative Constraints

- Do not persist savepoints across COMMIT
- Do not implement XA (M162)

## Test plan

```bash
cargo test -p rusql-executor savepoint
cargo test -p rusql-storage savepoint
cargo test -p rusql-server savepoint
```

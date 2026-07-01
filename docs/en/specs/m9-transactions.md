# M9: Transactions (BEGIN / COMMIT / ROLLBACK)

## Goal

Per-connection explicit transactions with read-your-writes and isolation from other connections until COMMIT.

## Acceptance criteria

- [ ] `BEGIN` / `START TRANSACTION` returns OK and starts a transaction
- [ ] DML inside a transaction is visible to the same connection only until COMMIT
- [ ] `COMMIT` persists pending changes to WAL and shared storage
- [ ] `ROLLBACK` discards uncommitted changes
- [ ] Compat fixture `transactions` suite passes
- [ ] User guide (en + zh-CN) updated

## File boundaries

- `crates/rusql-storage/src/txn.rs`
- `crates/rusql-storage/src/persistent.rs`
- `crates/rusql-server/src/connection.rs`
- `crates/rusql-server/compat/basic.json`
- `docs/en/user-guide.md`, `docs/zh-CN/user-guide.md`
- `docs/en/specs/m9-transactions.md`

## Negative constraints

- No MVCC across connections beyond commit boundary
- No savepoints, no `BEGIN WORK` modifiers
- No COM_STMT_* in this milestone

## Test strategy

- Unit: `rusql-storage` txn overlay commit/rollback
- Integration: `transaction_commit_and_rollback` wire test
- Compat: JSON `transactions` suite

## Autonomous decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Isolation model | Connection-local overlay + deferred WAL | Matches shared `PersistentEngine` architecture |
| Autocommit | Default outside explicit txn | MySQL default behavior |

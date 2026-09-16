## Goal

Accept `SELECT … FOR UPDATE` and `SELECT … FOR SHARE` as documented no-ops that return the same rows as the unlocked `SELECT`, so JDBC/ORM pessimistic-lock probes succeed instead of failing as an unsupported statement.

## Background

Phase Q after M84. sqlparser already attaches `Query.locks`; rusql ignores them today, but this is untested and undocumented. Connectors and ORMs (Hibernate, MyBatis, jOOQ) emit `FOR UPDATE` after `BEGIN`. rusql stays snapshot-isolation; this slice is parse/execute probe only (no row locks, no wait). `SHOW STATUS` stubs and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SELECT id FROM t FOR UPDATE` returns the same rows as `SELECT id FROM t` on that connection
- [ ] `SELECT id FROM t FOR SHARE` returns the same rows as the unlocked `SELECT`
- [ ] `SELECT … FOR UPDATE NOWAIT` and `SELECT … FOR UPDATE SKIP LOCKED` are accepted as the same no-op (no errno for “would wait”)
- [ ] Concurrent connections can both `SELECT … FOR UPDATE` the same row and both see it (no blocking; snapshot reads unchanged)
- [ ] `SET TRANSACTION ISOLATION LEVEL` overlays from M84 are unchanged. DML engine isolation stays snapshot
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` where Docker MySQL lock-wait/NOWAIT semantics differ; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (parse `FOR UPDATE` / `NOWAIT` / `SKIP LOCKED` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- Implementing real row locks, gap locks, or lock wait
- `SELECT … FOR UPDATE OF col` column-level locks (unless already parsed as table OF)
- `SHOW STATUS` stubs
- Changing MVCC/snapshot engine behavior
- New crates

## Negative Constraints

- Do not implement InnoDB-style exclusive/shared row locks or blocking
- Do not change autocommit / `BEGIN` engine behavior beyond accepting the lock clause
- Do not implement `GET_LOCK()` / advisory locks

## Test plan

```bash
cargo test -p rusql-executor for_update
cargo test -p rusql-server for_update
```

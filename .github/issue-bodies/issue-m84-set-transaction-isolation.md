## Goal

Wire `SET TRANSACTION ISOLATION LEVEL` (and `SET SESSION TRANSACTION ISOLATION LEVEL`) to the existing `@@transaction_isolation` / `@@tx_isolation` session overlays so JDBC/ORM isolation probes persist on that connection instead of failing as an unsupported statement.

## Background

Phase Q after M83. `SET @@transaction_isolation` already overlays the stub (hyphenated MySQL values such as `REPEATABLE-READ`). Connectors also send `SET TRANSACTION ISOLATION LEVEL READ COMMITTED` / `REPEATABLE READ`. rusql remains snapshot-isolation for DML; this slice is probe/persistence of the stub value only. GTID event type 33 / heartbeat stays later.

## Acceptance Criteria

- [ ] `SET TRANSACTION ISOLATION LEVEL READ COMMITTED` then `SELECT @@transaction_isolation` and `SELECT @@tx_isolation` return `READ-COMMITTED` on that connection
- [ ] `SET SESSION TRANSACTION ISOLATION LEVEL REPEATABLE READ` restores `REPEATABLE-READ` on that connection
- [ ] `SET TRANSACTION ISOLATION LEVEL SERIALIZABLE` and `READ UNCOMMITTED` overlay `SERIALIZABLE` and `READ-UNCOMMITTED` respectively
- [ ] `SHOW VARIABLES` / `SHOW SESSION VARIABLES` reflect the overlay; other connections keep documented defaults
- [ ] Assignments are in-memory per session (not WAL). `COM_RESET_CONNECTION` / `COM_CHANGE_USER` restore documented defaults
- [ ] `SET GLOBAL TRANSACTION …` is rejected (session-only this slice; reuse errno 1229 / existing i18n if it fits)
- [ ] Engine isolation is unchanged (still snapshot). `SET NAMES`, `SET CHARACTER SET`, and `@foo :=` from M82/M83 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` where Docker MySQL next-transaction vs session semantics or display names differ; docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (parse `SET TRANSACTION` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-core/src/**` (only if next-transaction vs session needs extra fields; prefer session overlay only)
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- Changing MVCC/snapshot engine behavior to honor isolation levels
- `SET TRANSACTION READ ONLY` / `READ WRITE` (unless a one-line no-op OK)
- `SET GLOBAL TRANSACTION` persistence
- Persisting variables across server restart
- New crates

## Negative Constraints

- Do not implement MySQL next-transaction-only vs session scope; treat `SET TRANSACTION` and `SET SESSION TRANSACTION` as the same in-memory session overlay and document that
- Do not implement `SET PASSWORD` / `SET ROLE` / other SET forms
- Do not change autocommit engine behavior

## Test plan

```bash
cargo test -p rusql-executor set_transaction
cargo test -p rusql-server set_transaction
```

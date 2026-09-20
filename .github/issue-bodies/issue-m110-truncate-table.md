## Goal

Accept `TRUNCATE TABLE` so clients that empty a table without `DELETE` get MySQL-shaped OK instead of `unsupported statement: Truncate`.

## Background

Phase Q after event catalog slices. sqlparser already produces `Statement::Truncate`. Probe (`scripts/mysql-gap-probe.mjs`, 2026-09-20): `truncate_table` → unsupported 1105. MySQL `TRUNCATE` deletes all rows and resets `AUTO_INCREMENT`; it is DDL (implicit commit). rusql has no separate DDL txn model — document that this slice empties the heap and resets the auto-increment counter when present.

## Acceptance Criteria

- [ ] `TRUNCATE TABLE t` (and `TRUNCATE t`) removes all rows from `t` and returns OK
- [ ] If `t` has `AUTO_INCREMENT`, the next generated id is 1 (counter reset)
- [ ] Unknown table is errno 1146. `TRUNCATE` of `information_schema` / virtual tables is rejected
- [ ] Existing `DELETE` / `DROP TABLE` behavior is unchanged
- [ ] Unit/wire tests; `mysql-diff` (row count 0 after truncate can compare); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-storage/src/**` (only if truncate needs an engine helper already used by delete-all)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout beyond existing DELETE/QUERY path
- `TRUNCATE` partitions / `CASCADE`
- New crates
- `REPLACE INTO` / `INSERT IGNORE` (separate issues)

## Negative Constraints

- Do not claim InnoDB tablespace reuse; heap delete-all + counter reset is enough
- Do not add trigger firing on TRUNCATE unless already required for DELETE ALL (MySQL does not fire DELETE triggers on TRUNCATE — do not fire them)
- Do not implement GTID event type 33 / heartbeat

## Test plan

```bash
cargo test -p rusql-executor truncate
cargo test -p rusql-server truncate
```

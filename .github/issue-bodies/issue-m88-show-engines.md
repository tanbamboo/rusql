## Goal

Implement `SHOW ENGINES` / `SHOW STORAGE ENGINES` so clients and GUIs that probe the storage-engine list get MySQL-shaped rows instead of an unsupported-statement error.

## Background

Phase Q after M87. sqlparser 0.53 has no `Statement::ShowEngines`; `SHOW ENGINES` typically lands as `ShowVariable` or a parse rewrite (same pattern as `SHOW PROCESSLIST` / `SHOW TABLE STATUS`). phpMyAdmin, the official CLI, and several GUIs send `SHOW ENGINES` or `SHOW STORAGE ENGINES` after table-status probes. This slice is a documented stub catalog: rusql does not switch engines. `SHOW ENGINE INNODB STATUS` and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW ENGINES` returns MySQL-shaped columns `Engine`, `Support`, `Comment`, `Transactions`, `XA`, `Savepoints`
- [ ] The documented stub set includes at least `InnoDB` with `Support` = `DEFAULT` (matches M87 `SHOW TABLE STATUS` `Engine` stub)
- [ ] Other documented stub engines (if any) use `YES`/`NO` for `Support` as in MySQL; values are constants, not live plugins
- [ ] `SHOW STORAGE ENGINES` is equivalent to `SHOW ENGINES` for this slice
- [ ] `SHOW TABLE STATUS` stubs from M87 and `SHOW STATUS` stubs from M86 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (engine list/comments differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW ENGINES` / `SHOW STORAGE ENGINES` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- Real engine plugins, tablespace files, or `ENGINE=` switching
- Full MySQL 8.0 engine catalog
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW ENGINES` parity; document the stub subset
- Do not implement `ENGINE=MyISAM` (or other engines) on `CREATE TABLE`
- Do not implement `SHOW ENGINE … STATUS` (InnoDB mutex/log dump)
- Do not implement `WHERE` filtering unless sqlparser already exposes it as a one-line wire-up

## Test plan

```bash
cargo test -p rusql-sql show_engines
cargo test -p rusql-executor show_engines
cargo test -p rusql-server show_engines
```

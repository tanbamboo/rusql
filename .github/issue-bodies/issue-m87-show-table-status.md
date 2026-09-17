## Goal

Implement `SHOW TABLE STATUS` so clients and GUIs that probe table metadata get one row per table in the current database (MySQL-shaped columns) instead of an unsupported-statement error.

## Background

Phase Q after M86. sqlparser 0.53 has no `Statement::ShowTableStatus`; `SHOW TABLE STATUS` typically lands as `ShowVariable` or a parse rewrite (same pattern as `SHOW PROCESSLIST`). Official CLI, phpMyAdmin, and several ORMs send `SHOW TABLE STATUS` / `SHOW TABLE STATUS LIKE '…'`. This slice is a documented stub: real table **names** from the current schema; other cells are stubs (not InnoDB row-format/stats). `SHOW ENGINE INNODB STATUS` and GTID event type 33 / heartbeat stay later.

## Acceptance Criteria

- [ ] `SHOW TABLE STATUS` returns MySQL-shaped columns including at least `Name`, `Engine`, `Version`, `Row_format`, `Rows`, `Avg_row_length`, `Data_length`, `Max_data_length`, `Index_length`, `Data_free`, `Auto_increment`, `Create_time`, `Update_time`, `Check_time`, `Collation`, `Checksum`, `Create_options`, `Comment`
- [ ] One row per base table in the current database (`USE`); `Name` matches `SHOW TABLES`
- [ ] `Engine` is the documented stub `InnoDB` (not a real engine switch)
- [ ] Other numeric/time/comment cells are documented stubs (`0` / empty) unless a one-line existing catalog field is already available (`Rows` MAY be the current heap row count; `Auto_increment` MAY match the table counter)
- [ ] `SHOW TABLE STATUS LIKE 't%'` (MySQL `LIKE` on `Name`) filters that set; a non-matching pattern returns zero rows, not an error
- [ ] `SHOW STATUS` stubs from M86 are unchanged
- [ ] Unit/wire tests; `mysql-diff` with `compare_output: false` (row stats differ from Docker MySQL); docs: CHANGELOG, release-notes, user-guide (en+zh-CN), HANDOFF

## File Boundaries

Allowed:
- `crates/rusql-executor/src/**`
- `crates/rusql-sql/src/**` (rewrite/parse `SHOW TABLE STATUS [FROM db] [LIKE …]` if needed)
- `crates/rusql-server/src/**`
- `crates/rusql-i18n/**` (errors only)
- `crates/rusql-server/compat/mysql-diff.json`
- `CHANGELOG.md`, `docs/en/**`, `docs/zh-CN/**`, `HANDOFF.md`
- `.github/issue-bodies/**`

Forbidden:
- Changing WAL JSON format
- Replication event layout
- `SHOW ENGINE INNODB STATUS` / `SHOW ENGINE MUSQL STATUS`
- Full `information_schema.TABLES` parity beyond this SHOW
- Live InnoDB tablespace stats
- New crates

## Negative Constraints

- Do not claim full MySQL 8.0 `SHOW TABLE STATUS` parity; document the stub cells
- Do not implement `WHERE` filtering unless sqlparser already exposes it as a one-line wire-up (`LIKE` is required)
- Do not implement `SHOW TABLE STATUS FROM other_db` unless the rewrite is a small addition; current-database listing is required

## Test plan

```bash
cargo test -p rusql-sql table_status
cargo test -p rusql-executor table_status
cargo test -p rusql-server table_status
```

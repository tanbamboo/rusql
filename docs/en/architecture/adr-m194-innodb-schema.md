# ADR: M194 `information_schema.INNODB_*` stubs

**Status**: Accepted (M194 / [#347](https://github.com/tanbamboo/rusql/issues/347))  
**Date**: 2026-10-28

## Context

Monitors query `information_schema.INNODB_TRX` and `information_schema.INNODB_LOCKS`. Those names are not tables today, so the statement is errno **1146**. rusql is not Oracle InnoDB. It already records explicit transactions and their row/gap locks for `SHOW ENGINE INNODB STATUS` (M193). This slice exposes that same registry as two virtual tables. It does not create tablespaces (M198) and it does not change `CONSTITUTION.md`.

MySQL 8.0 still has `INNODB_TRX`. It removed `INNODB_LOCKS` (the replacement is `performance_schema.data_locks`, which stays errno **1146** here). The issue asks for the monitoring names, so rusql keeps `INNODB_LOCKS`.

## Decision

1. **Virtual tables, not stored objects.** Qualified `SELECT` on the two names below reads the open-transaction registry. Nothing is written to the catalog or the WAL. `INSERT`, `UPDATE`, and `DELETE` still see no base table (errno **1146**). A real base table with the same storage key, if one was created, is used instead of the stub. Letter case does not matter.

2. **Implemented tables.**

   | Table | Rows | Columns (`SELECT *` order) |
   |-------|------|------------------------------|
   | `INNODB_TRX` | One row per explicit rusql transaction (`BEGIN`, or `SAVEPOINT` when it opens a transaction). `COMMIT`, `ROLLBACK`, reset, and disconnect remove it. Autocommit statements are not left in the list. | `TRX_ID`, `TRX_STATE`, `TRX_STARTED`, `TRX_REQUESTED_LOCK_ID`, `TRX_WAIT_STARTED`, `TRX_MYSQL_THREAD_ID`, `TRX_QUERY`, `TRX_ROWS_LOCKED`, `TRX_ROWS_MODIFIED`, `TRX_ISOLATION_LEVEL` |
   | `INNODB_LOCKS` | One row per row or gap lock held by those transactions. Zero rows when nothing is held. | `LOCK_ID`, `LOCK_TRX_ID`, `LOCK_MODE`, `LOCK_TYPE`, `LOCK_TABLE`, `LOCK_INDEX`, `LOCK_SPACE`, `LOCK_PAGE`, `LOCK_REC`, `LOCK_DATA` |

   `SELECT *` succeeds with zero or more rows. It is not errno **1146**.

3. **What each cell is.** `TRX_ID` and `LOCK_TRX_ID` are the rusql listener-local decimal, the same number as `---TRANSACTION {id}` in `SHOW ENGINE INNODB STATUS`. They are not InnoDB transaction ids. `TRX_STATE` is `RUNNING` (a blocked transaction is not reported as `LOCK WAIT`). `TRX_MYSQL_THREAD_ID` is the connection id. `TRX_ROWS_LOCKED` counts record-lock notes. `TRX_ROWS_MODIFIED` is the constant `0`. `TRX_STARTED`, `TRX_REQUESTED_LOCK_ID`, `TRX_WAIT_STARTED`, `TRX_QUERY`, and `TRX_ISOLATION_LEVEL` are SQL NULL. No waited lock id is invented.

   `LOCK_ID` is `rusql:{trx_id}:{ordinal}` with `ordinal` starting at 0 in note order. That string is not an InnoDB `trx:space:page:heap` id. `LOCK_MODE` is `X` for a record lock and `X,GAP` for a gap lock. `LOCK_TYPE` is `RECORD`. `LOCK_TABLE` is the table name on the lock note (no schema). `LOCK_DATA` is the primary-key text for a record lock, or `{low}/{high}` for a gap (`-` when a bound is open). `LOCK_INDEX`, `LOCK_SPACE`, `LOCK_PAGE`, and `LOCK_REC` are SQL NULL. Page, space, and heap numbers are not invented.

4. **Everything else named `INNODB_*` is errno 1146.** That includes `INNODB_LOCK_WAITS`, `INNODB_METRICS`, `INNODB_BUFFER_PAGE`, `INNODB_TABLES`, and every `INNODB_SYS_*` name. This slice does not add `SYS_*` tables.

5. **Unknown column.** A name that is not in the table's column list, including `TRX_WEIGHT`, is errno **1054**.

6. **Privilege.** `root` can `SELECT`. Any other account needs `SELECT` on that table (or a broader grant). Without it the statement is errno **1142**. This is not MySQL's `PROCESS` filter.

7. **mysql-diff.** `INNODB_TRX` is compared on a filter that is empty on both servers (`TRX_ID = '___none___'`), plus `INNODB_LOCK_WAITS` and `INNODB_SYS_TABLES` (errno **1146** on both). `INNODB_LOCKS` is not in mysql-diff, because MySQL 8.0 returns **1146** for that name and rusql does not.

## Consequences

- Clients can `SELECT` `INNODB_TRX` and `INNODB_LOCKS` without errno 1146. An idle server returns zero rows.
- Lock ids cannot be parsed as InnoDB lock ids. Tools that require `space`/`page`/`heap` see NULL in those columns.
- `INNODB_LOCKS` succeeding is a documented difference from MySQL 8.0.

## Out of scope

- The rest of the MySQL 8.0 `INNODB_*` catalog (`INNODB_METRICS`, buffer-pool tables, `INNODB_TABLES`, and so on)
- `INNODB_SYS_*` tables
- `performance_schema.data_locks` (still errno **1146**)
- InnoDB tablespaces and crash recovery (M198)
- Fake mutex, buffer-pool, or lock-memory counters
- Claiming MySQL 8.0 compatibility is complete

## How to verify

```bash
cargo test -p rusql-executor innodb_trx
cargo test -p rusql-server innodb_trx
```

```sql
BEGIN;
SELECT id FROM t WHERE id = 1 FOR UPDATE;
SELECT TRX_ID, TRX_STATE, TRX_MYSQL_THREAD_ID FROM information_schema.INNODB_TRX;
SELECT LOCK_ID, LOCK_MODE, LOCK_DATA FROM information_schema.INNODB_LOCKS;
COMMIT;
```

While the transaction is open, `INNODB_TRX` has `RUNNING` and that connection id, and `INNODB_LOCKS`.`LOCK_ID` starts with `rusql:`. After `COMMIT` those rows are gone. `SELECT * FROM information_schema.INNODB_SYS_TABLES` is errno **1146**. `SELECT TRX_WEIGHT FROM information_schema.INNODB_TRX` is errno **1054**.

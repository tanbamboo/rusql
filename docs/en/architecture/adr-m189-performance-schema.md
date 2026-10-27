# ADR: M189 `performance_schema` stub

**Status**: Accepted (M189 / [#341](https://github.com/tanbamboo/rusql/issues/341))  
**Date**: 2026-10-27

## Context

Connectors probe `performance_schema` (`session_variables`, `accounts`, statement digests, `threads`). Today those names are not tables, so the statement is errno **1146**. MySQL 8.0's performance schema is a large instrumented runtime. Turning every consumer on, or writing digest rows into the WAL, is outside this slice and would spend CPU on counters this server does not collect.

This ADR is the queryable stub. It does not change `CONSTITUTION.md`. It is not full instrument parity, not live `SHOW STATUS` (M190), and not `information_schema.INNODB_*` (M194).

## Decision

1. **Virtual tables, not stored objects.** Qualified `SELECT` on the names below reads an in-memory stub. Nothing is written to the catalog or the WAL. `INSERT`, `UPDATE`, and `DELETE` still see no base table (errno **1146**). A real base table with the same storage key, if one was created, is used instead of the stub. `USE performance_schema` is not a stored database. `SHOW DATABASES` does not list it.

2. **Implemented tables.** Letter case does not matter (`PERFORMANCE_SCHEMA.SESSION_VARIABLES` is the same table).

   | Table | Rows | Columns (`SELECT *` order) |
   |-------|------|------------------------------|
   | `session_variables` | The existing `SHOW SESSION VARIABLES` stub. Session `SET` overlays apply. | `VARIABLE_NAME` `varchar(64)`, `VARIABLE_VALUE` `varchar(1024)` |
   | `global_variables` | The same names, without the session overlay (global defaults, including `@@global.gtid_executed`). | same two columns |
   | `accounts` | Zero rows. Counter columns are not incremented. | `USER` `char(32)`, `HOST` `char(255)`, `CURRENT_CONNECTIONS` `bigint unsigned`, `TOTAL_CONNECTIONS` `bigint unsigned` |
   | `events_statements_summary_by_digest` | Zero rows. This is the `events_statements_digest` probe. Digests are not collected and are not persisted. | `SCHEMA_NAME`, `DIGEST`, `DIGEST_TEXT`, `COUNT_STAR`, `SUM_ERRORS`, `SUM_WARNINGS`, `SUM_ROWS_AFFECTED`, `SUM_ROWS_SENT`, `SUM_ROWS_EXAMINED`, `FIRST_SEEN`, `LAST_SEEN` |
   | `threads` | Zero rows. Not a live thread list. | `THREAD_ID`, `NAME`, `TYPE`, `PROCESSLIST_ID`, `PROCESSLIST_USER`, `PROCESSLIST_HOST`, `PROCESSLIST_DB`, `PROCESSLIST_COMMAND`, `PROCESSLIST_INFO`, `INSTRUMENTED` |

   `SELECT *` on these tables succeeds with zero or more rows. It is not errno **1146**.

3. **Everything else in the schema is errno 1146.** That includes `events_waits_current`, `setup_instruments`, `setup_consumers`, `session_status`, and `global_status`. Consumers stay off. There is no setup table that turns instruments on. `session_status` / `global_status` stay on M190 (`SHOW STATUS`), not this stub.

4. **Unknown column.** A name that is not in the table's column list is errno **1054**. The variable tables are only the documented `SHOW VARIABLES` stub, not the full MySQL 8.0 catalog.

5. **Privilege.** `root` can `SELECT`. Any other account needs `SELECT` on that table (or a broader grant). Without it the statement is errno **1142**.

## Consequences

- Clients can `SELECT` the five tables above without errno 1146. `threads` is an empty stub, not a silent 1146.
- Counter columns on `accounts` and `events_statements_summary_by_digest` exist and stay at zero rows because instruments are off.
- `mysql-diff` compares column names and filters that are empty on both servers, plus `VARIABLE_NAME = 'autocommit'`. It does not compare the full variable list to MySQL.

## Out of scope

- Full Performance Schema instrument parity, consumers, and history tables
- Persisting digests or thread rows
- Enabling instruments by default
- Live `SHOW STATUS` (M190) and `performance_schema.session_status` / `global_status`
- InnoDB mutex data (M193) and `information_schema.INNODB_*` (M194)
- Claiming MySQL 8.0 compatibility is complete

## How to verify

```bash
cargo test -p rusql-executor performance_schema
cargo test -p rusql-server performance_schema
```

```sql
SELECT VARIABLE_NAME, VARIABLE_VALUE
  FROM performance_schema.session_variables
  WHERE VARIABLE_NAME = 'autocommit';
SELECT * FROM performance_schema.accounts;
SELECT * FROM performance_schema.events_statements_summary_by_digest;
SELECT THREAD_ID FROM performance_schema.threads;
```

`autocommit` is `1` until `SET @@autocommit = 0`, which changes only `session_variables`. `accounts`, the digest table, and `threads` return their columns and zero rows. `SELECT * FROM performance_schema.events_waits_current` is errno **1146**.

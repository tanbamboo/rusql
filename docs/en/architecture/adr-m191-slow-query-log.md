# ADR: M191 slow query log

**Status**: Accepted (M191 / [#343](https://github.com/tanbamboo/rusql/issues/343))  
**Date**: 2026-10-28

## Context

Operators need a file of statements that ran longer than a threshold. MySQL 8.0 does that with `slow_query_log`, `slow_query_log_file`, and `long_query_time`, plus `--slow-query-log` and `--slow-query-log-file`. The log is off unless it is turned on. A threshold of `0` records every qualifying statement.

This slice writes that file. It does not change `CONSTITUTION.md`. It is not the general log (M192), not `performance_schema` digests (M189), and not a live `Slow_queries` status counter (that name stays the M190 constant `0`).

## Decision

1. **Off by default.** No flag and `slow_query_log` left at `0` means rusql does not create or append the file. `--slow-query-log` turns it on at startup. `--slow-query-log-file=PATH` sets the path (default `{data-dir}/rusql-slow.log`). `--long-query-time=SECONDS` sets the global threshold (default `10`, displayed as `10.000000`). A negative or non-finite startup value stops the process with errno text from `sql.wrong_value_for_var`. A path that cannot be opened when the log starts enabled stops the process with `server.slow_query_log_open_failed`.

2. **Variables.**
   - `long_query_time` is session and global. `SET SESSION`, `SET @@session`, `SET @@`, and bare `SET` store the session value. Until the session sets one, `@@long_query_time` follows the global value, including a `SET GLOBAL` on this connection. After a session value is set, `SET GLOBAL` does not replace it. `SELECT` shows six decimal places (`0.000000`). A numeric value outside `[0, 31536000]` is stored clamped to that range (MySQL also returns OK; rusql does not emit warning 1292 because `SHOW WARNINGS` is still empty). A non-numeric value is errno **1231**. `--long-query-time` still rejects a negative or non-finite startup value.
   - `slow_query_log` and `slow_query_log_file` are global only. `SET GLOBAL` / `SET @@global` updates them. `SET SESSION` is errno **1229**. `SELECT @@session.slow_query_log` (and the file variable) is errno **1238**. `@@slow_query_log` is `0` or `1` (not `OFF` / `ON`). An empty path is errno **1231**.
   - `SHOW VARIABLES` lists the three names. Session `long_query_time` is the session value. The other two show the global value in both session and global lists.

3. **What is written.** After a finished `COM_QUERY`, if the log is enabled and the elapsed time is at least the effective `long_query_time` (microseconds, so `0` logs every non-empty packet), rusql appends one entry. The comparison uses the threshold in effect after the packet, so `SET SESSION long_query_time = 0` itself is logged when the log is already on. A multi-statement packet is one entry containing the whole packet text. `COM_STMT_EXECUTE`, `COM_CHANGE_USER`, and other commands are not entries. A delimiter-only packet is not an entry.

4. **File lines (subset).** No startup banner, no `Lock_time`, no `Rows_sent`, no `Rows_examined`:

   ```
   # Time: 2026-10-28T00:00:00.000Z
   # User@Host: root @ 127.0.0.1
   # Query_time: 0.000012
   SELECT 1
   ```

   `Time` is UTC with millisecond precision. `Query_time` is seconds with six decimals. The statement is one physical line.

5. **Passwords.** `COM_CHANGE_USER` is not written, so its password packet is not in the file. In `COM_QUERY` text, the same literals as the M185 audit log (`IDENTIFIED BY`, `IDENTIFIED BY PASSWORD`, `IDENTIFIED WITH … BY|AS`, `SET PASSWORD … =`, `PASSWORD(`) become `'<redacted>'`. That token is part of the file format in every locale.

6. **Write errors do not change the client result.** A full disk logs a server warning. The statement still returns its normal errno.

## Consequences

- `Slow_queries` in `SHOW STATUS` stays `0`.
- Turning the log on does not create the file until a statement is actually logged.
- `mysql-diff` `slow_query_log` compares `@@long_query_time`, `@@slow_query_log`, session `SET`, and the global-only errnos. It does not read the file and does not `SET GLOBAL` on the reference server. The file check is `cargo test -p rusql-server slow_query_log`.
- `performance_schema.session_variables` / `global_variables` include these three names because they reuse `SHOW VARIABLES`.

## Out of scope

- General log (M192) and `--log-error` (M195)
- `log_queries_not_using_indexes`, `min_examined_row_limit`, `log_slow_extra`, `log_throttle_queries_not_using_indexes`
- Microsecond extra columns (`Lock_time`, `Rows_sent`, `Rows_examined`)
- Logging `COM_STMT_EXECUTE` or `COM_CHANGE_USER`
- A live `Slow_queries` counter
- `performance_schema` statement digests
- Claiming MySQL 8.0 compatibility is complete

## How to verify

```bash
cargo test -p rusql-executor slow_query_log
cargo test -p rusql-server slow_query_log
```

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data --slow-query-log --long-query-time 0
```

```sql
SET GLOBAL slow_query_log = ON;
SET SESSION long_query_time = 0;
SELECT 1;
```

`{data-dir}/rusql-slow.log` contains `SELECT 1`. Starting without `--slow-query-log` and without `SET GLOBAL slow_query_log = ON` does not create the file.

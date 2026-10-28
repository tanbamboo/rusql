# ADR: M192 general query log

**Status**: Accepted (M192 / [#344](https://github.com/tanbamboo/rusql/issues/344))  
**Date**: 2026-10-28

## Context

Operators need an optional file of connections and statements. MySQL 8.0 does that with `general_log` and `general_log_file`, plus `--general-log` and `--general-log-file`. The log is off unless it is turned on. This slice writes that file. It does not change `CONSTITUTION.md`. It is not the slow query log (M191), not the M185 audit JSON file, and not `log_output = TABLE`.

## Decision

1. **Off by default.** No flag and `general_log` left at `0` means rusql does not create or append the file. `--general-log` turns it on at startup. `--general-log-file=PATH` sets the path (default `{data-dir}/rusql-general.log`) and does not by itself turn the log on. A path that cannot be opened when the log starts enabled stops the process with `server.general_log_open_failed`. An empty path does the same.

2. **Variables.** `general_log` and `general_log_file` are global only. `SET GLOBAL` / `SET @@global` updates them. `SET SESSION` is errno **1229**. `SELECT @@session.general_log` (and the file variable) is errno **1238**. `@@general_log` is `0` or `1` (not `OFF` / `ON`). `ON` / `OFF` / `1` / `0` are accepted on `SET GLOBAL`. An empty path, or a path with a control character, is errno **1231**. `SHOW VARIABLES` lists both names. Session and global lists both show the global value.

3. **What is written when the log is on.**
   - After a successful login, one `Connect` line (`user@host` and ` on {database}` when the client selected one).
   - After a finished `COM_QUERY`, one `Query` line with the packet text. The check uses the flag after the packet, so `SET GLOBAL general_log = ON` itself is logged. A multi-statement packet is one line. SQL errors are still logged. A delimiter-only empty packet is not a line.
   - On `COM_QUIT`, one `Quit` line. An abrupt disconnect is not a line.
   - `COM_STMT_EXECUTE`, `COM_CHANGE_USER`, and other commands are not lines.

4. **File lines (subset).** No startup banner and no result rows. Fields are separated by tabs:

   ```
   2026-10-28T00:00:00.000Z	1	Connect	root@127.0.0.1 on rusql
   2026-10-28T00:00:00.000Z	1	Query	SELECT 1
   2026-10-28T00:00:00.000Z	1	Quit
   ```

   `Time` is UTC with millisecond precision. The statement is one physical line. `Connect`, `Query`, and `Quit` are file-format tokens in every locale.

5. **Passwords.** The same literals as the M185 audit log and the M191 slow log (`IDENTIFIED BY`, `IDENTIFIED BY PASSWORD`, `IDENTIFIED WITH … BY|AS`, `SET PASSWORD … =`, `PASSWORD(`) become `'<redacted>'`. That token is part of the file format in every locale. `COM_CHANGE_USER` is not written, so its password packet is not in the file. A value that exists only in a result row is not written.

6. **Write errors do not change the client result.** A full disk logs a server warning. The statement still returns its normal errno. This file is not the M185 JSON audit log. Turning the log on in default CI is not required; mysql-diff does not `SET GLOBAL general_log = ON` on the reference server.

## Consequences

- The slow query log is unchanged. A statement can be in one file, both, or neither.
- `mysql-diff` `general_log` compares `@@general_log` and the global-only errnos. It does not read the file and does not compare `@@general_log_file` (MySQL's default path is the host name). The file check is `cargo test -p rusql-server general_log`.
- `performance_schema.session_variables` / `global_variables` include these two names because they reuse `SHOW VARIABLES`.

## Out of scope

- `log_output = TABLE` / `mysql.general_log`
- Logging result sets, `COM_STMT_EXECUTE`, or `COM_CHANGE_USER`
- Log rotation, the error log (M195), and a startup banner
- Claiming MySQL 8.0 compatibility is complete

## How to verify

```bash
cargo test -p rusql-executor general_log
cargo test -p rusql-server general_log
```

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data --general-log
```

```sql
SET GLOBAL general_log = ON;
SELECT 1;
```

`{data-dir}/rusql-general.log` contains `Connect` for a login made while the log is on, and `Query` plus `SELECT 1`. Starting without `--general-log` and without `SET GLOBAL general_log = ON` does not create the file.

# ADR: M195 error log `--log-error`

**Status**: Accepted (M195 / [#348](https://github.com/tanbamboo/rusql/issues/348))  
**Date**: 2026-10-28

## Context

Operators need server errors in a file. MySQL 8.0 does that with `--log-error` / `--log_error` and the read-only global variable `log_error`. Without the flag, messages stay on stderr. This slice writes the file. It does not change `CONSTITUTION.md`. It is not the general log (M192), not the slow query log (M191), and not the M185 audit JSON file.

## Decision

1. **Off by default.** No `--log-error` means rusql does not create or append a file. Stderr tracing is the same subscriber as before (`EnvFilter` from the environment). The default level is not raised to debug. `@@log_error` and `@@global.log_error` are the token `stderr`.

2. **Flag.** `--log-error=PATH` and `--log_error=PATH` are the same option. The process opens that path at startup (append, create). An empty path, or a path that cannot be opened, stops the process with `server.error_log_open_failed`. A later startup failure (storage, privileges, bind, TLS material, the other log files) appends one `Error` line and then exits. A successful bind appends one `System` line using the already localized `server.starting` text, including the port.

3. **What is appended when the file is open.**
   - A failed login (unknown account, bad password, plaintext `REQUIRE SSL`) is one `Error` line with errno **1045** and the localized access-denied text. The password bytes are not written.
   - Each SQL ERR packet on an established connection is one `Error` line with that errno and the packet message. A successful statement is not a line. `SELECT 1` does not appear because it succeeded.
   - Other connection failures that are not the access-denied text are one `Error` line with errno `0` and the redacted message.
   - A full disk logs a server warning. The client still gets its normal errno.

4. **File lines (subset).** Fields are separated by tabs. `System` and `Error` are file-format tokens in every locale. Connection id `0` is startup. There is no `log_error_verbosity` matrix and no rotation.

   ```
   2026-10-28T00:00:00.000Z	0	System	Starting rusql server on port 3307
   2026-10-28T00:00:00.000Z	8	Error	1045	Access denied for user
   2026-10-28T00:00:00.000Z	8	Error	1146	Table 'no_such' doesn't exist
   ```

   `Time` is UTC with millisecond precision. The message is one physical line.

5. **Variable.** `log_error` is global and read-only. `SELECT @@log_error` / `@@global.log_error` return the startup path, or `stderr` when the flag is omitted. `SET GLOBAL` is errno **1238**. `SET` and `SET SESSION` are errno **1229** (the statement parser does not mark a bare `SET` as `GLOBAL`, so both are rejected as a session assignment of a global variable). `SELECT @@session.log_error` is errno **1238**. `SHOW VARIABLES` lists the name. The path cannot be changed while the process is running.

6. **Passwords.** The same literals as the M185 audit log (`IDENTIFIED BY`, `IDENTIFIED BY PASSWORD`, `IDENTIFIED WITH … BY|AS`, `SET PASSWORD … =`, `PASSWORD(`) become `'<redacted>'` inside an error message. That token is part of the file format in every locale. Authentication packets are not copied. `--auth-password` is not written.

## Consequences

- The general log, slow log, and audit file are unchanged. A failed statement can be in the error log and also in those files when they are on.
- `mysql-diff` `log_error` compares the read-only and global-only failures. It does not compare `@@log_error` text (MySQL's value is the host's path or `stderr`). The file check is `cargo test -p rusql-server log_error`.
- `performance_schema.global_variables` includes `log_error` because it reuses `SHOW VARIABLES`.

## Out of scope

- `log_error_verbosity`, `log_error_services`, and the JSON error-log sink
- Log rotation and a timestamp time zone other than UTC
- Logging result rows or successful statements
- Claiming MySQL 8.0 compatibility is complete

## How to verify

```bash
cargo test -p rusql-executor log_error
cargo test -p rusql-server log_error
```

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data --log-error ./rusql-data/rusql.err
```

A failed login and `SELECT * FROM no_such_table` append `Error` lines. Starting without `--log-error` does not create the file, and `@@log_error` is `stderr`.

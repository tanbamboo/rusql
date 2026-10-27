# ADR: M185 optional JSON audit log

**Status**: Accepted (M185 / [#337](https://github.com/tanbamboo/rusql/issues/337))  
**Date**: 2026-10-27

## Context

Operators sometimes need a record of who connected and which statements ran. MySQL Enterprise ships that as the closed `audit_log` plugin (XML or its own JSON schema, filters, and rotation). rusql does not load that plugin and does not claim to.

This slice is an opt-in file on the rusql server process. It is off unless the operator passes a path. It does not change `CONSTITUTION.md`. `performance_schema` stays on M189. Enterprise plugins stay on M188.

## Decision

1. **Flag `--audit-log=PATH`.** Omit it and rusql creates no audit file. Set it and the process creates or appends that file before accepting connections. A missing parent directory is created. An empty path or a path that cannot be opened stops the process with an i18n error. On success the process writes one stderr line: this log is not the Oracle MySQL Enterprise Audit plugin.

2. **JSON lines, four fields.** Each event is one JSON object and one line: `timestamp` (UTC `YYYY-MM-DDTHH:MM:SS.mmmZ`), `user`, `command`, `status`. There is no XML mode, no enterprise filter, and no rotation.

3. **What is recorded.**
   - After a successful handshake: `command` is `Connect`, `status` is 0.
   - Each finished `COM_QUERY`: one line. `command` is the statement text on one line. `status` is 0 when the response had no ERR packet, otherwise the MySQL errno from the first ERR packet (for example 1146).
   - When that session ends: `command` is `Quit`, `status` is 0. A successful `COM_CHANGE_USER` updates the user on this line.
   - A failed handshake is not a line. The authentication packet is not an audit field.

4. **Privacy.** Result-set rows are not written. `COM_STMT_EXECUTE` and other commands are not query lines. `COM_CHANGE_USER` is not a query line because its packet carries the password. In `COM_QUERY` text, the literal after `IDENTIFIED BY`, `IDENTIFIED BY PASSWORD`, `IDENTIFIED WITH … BY|AS`, `SET PASSWORD … =`, and `PASSWORD(` is replaced with `'<redacted>'`. That token is part of the file format in every locale.

5. **Write errors do not change the client result.** A full disk logs a server warning and the statement still returns its normal errno. The client does not see a new audit errno.

## Consequences

- The file can grow without bound. Rotation is out of this slice.
- `mysql-diff` stays a SQL comparison and does not read this file. The check is `cargo test -p rusql-server audit_log`.
- Statements that contain a password in the SQL text are stored only in redacted form. Values that appear only in a result row are not stored. A literal inside `INSERT` is the statement text, so it is stored once.

## Out of scope

- Oracle `audit_log` plugin, XML, query filters, and rotation
- Logging result rows, handshake passwords, or `COM_CHANGE_USER` payloads
- `performance_schema` (M189)
- `COM_STMT_EXECUTE` as its own audit event

## How to verify

```bash
cargo test -p rusql-server audit_log
```

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data --audit-log ./audit.jsonl
```

Connect, run `SELECT 1`, and disconnect. The file has a `Connect` line, a `SELECT 1` line with `"status":0`, and a `Quit` line. Starting without `--audit-log` does not create the file.

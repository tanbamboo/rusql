# ADR: M190 live `SHOW STATUS`

**Status**: Accepted (M190 / [#342](https://github.com/tanbamboo/rusql/issues/342))  
**Date**: 2026-10-27

## Context

M86 returns a small `SHOW STATUS` catalog with constant cells. Operators and connectors read `Questions`, `Uptime`, and `Threads_connected`. Those three can follow work this process already does: each `COM_QUERY`, the process start time, and the connection registry. The rest of the MySQL 8.0 status catalog (including `Com_*`) is hundreds of names and is not this slice.

This ADR does not change `CONSTITUTION.md`. It is not full status-variable parity, not `performance_schema.session_status` / `global_status` (those stay errno **1146** from M189), and not live InnoDB status (M193).

## Decision

1. **Three live names.** When the session has a connection registry (the server always attaches one):
   - `Questions` is the number of `COM_QUERY` packets since process start. The increment happens before the statement runs, so `SHOW STATUS LIKE 'Questions'` includes itself. A second such query is at least one higher.
   - `Uptime` is whole seconds since that registry was created (process start).
   - `Threads_connected` is the number of registered connections.
2. **One process-wide list.** `SHOW STATUS`, `SHOW SESSION STATUS`, and `SHOW GLOBAL STATUS` return the same names and the same values. This slice does not keep a separate per-session `Questions` counter.
3. **Other catalog names stay M86 constants.** `Threads_running` is `1`. `Connections` is `1`. `Aborted_connects`, `Bytes_received`, `Bytes_sent`, `Open_tables`, and `Slow_queries` are `0`. No `Com_*` names are added. `LIKE` is unchanged: a pattern that matches nothing, including `Com%`, returns zero rows.
4. **Columns.** `Variable_name` and `Value`, same as M86. Unknown `LIKE` is not an error.
5. **No registry.** An executor session with no process list keeps the M86 defaults (`Questions` `0`, `Uptime` `0`, `Threads_connected` `1`).
6. **`FLUSH STATUS` stays errno 1235** (M187) and does not zero these counters.

## Consequences

- Two `COM_QUERY` calls of `SHOW STATUS LIKE 'Questions'` show a higher `Questions` the second time.
- `Uptime` increases after at least one second of wall time.
- `Threads_connected` changes when another connection registers.
- mysql-diff compares success of `SHOW STATUS` / `LIKE 'Questions'` / `LIKE 'Uptime'`, not the numeric cells (MySQL's catalog is larger and the counters differ).

## Out of scope

- `Com_*` and the rest of the MySQL 8.0 status variable list
- Per-session versus global `Questions`
- `FLUSH STATUS` resetting counters
- `performance_schema.session_status` / `global_status`
- Live InnoDB status (M193) and `information_schema.INNODB_*` (M194)
- Claiming MySQL 8.0 compatibility is complete

## How to verify

```bash
cargo test -p rusql-executor show_status_live
cargo test -p rusql-server show_status_live
```

```sql
SHOW STATUS LIKE 'Questions';
SHOW SESSION STATUS LIKE 'Questions';
SHOW GLOBAL STATUS LIKE 'Uptime';
SHOW STATUS LIKE 'Threads_connected';
SHOW STATUS LIKE 'Slow_queries';
```

The second `Questions` value is greater than the first. `Slow_queries` stays `0`. `SHOW STATUS LIKE 'Com%'` returns zero rows.

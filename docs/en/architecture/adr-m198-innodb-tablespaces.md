# ADR: M198 crash recovery equivalent to InnoDB tablespaces

**Status**: Accepted (M198 / [#351](https://github.com/tanbamboo/rusql/issues/351))  
**Date**: 2026-10-29

## Context

MySQL 8.0 InnoDB recovers a crash from a redo log, a doublewrite buffer that absorbs torn pages, and tablespace files (`.ibd` when `innodb_file_per_table` is `ON`, which is the default). rusql already stores committed changes in an append-only JSON-line WAL (`{data_dir}/rusql.wal`) and keeps the table heap in memory. Restart replays that WAL. Calling that heap plus WAL an InnoDB tablespace would be a false claim. This issue requires the comparison, a test that data committed before a kill is present after open, and a rule that M210 cannot close without this ADR. This decision does not change `CONSTITUTION.md`.

## Decision

1. **Keep the rusql WAL.** Do not add `.ibd` files, an `ibdata` system tablespace, or an InnoDB redo file. Do not drop the WAL. `SHOW ENGINES` is unchanged: `InnoDB` stays the existing compatibility name and is not a tablespace implementation.

2. **Durability the client can see** (default `--wal-sync always`, `WalSyncPolicy::Always`):
   - `COMMIT` and an autocommit statement append their JSON lines and `fsync` the WAL before the call returns.
   - Killing the process after that return drops the memory heap. Opening the same data directory replays the WAL. Those committed rows are present.
   - A transaction that has not reached `COMMIT` is absent.
   - A trailing partial line with no newline is a torn write. Recovery discards it, truncates the file to the last complete line, and syncs. Earlier complete lines stay, and the next append is a new line.
   - A complete line that does not decode is corruption. Recovery fails and the file is not truncated.

3. **Comparison.** This is the equivalent guarantee, not file-format compatibility.

   | InnoDB 8.0 | rusql |
   |------------|--------|
   | Redo log | `{data_dir}/rusql.wal` JSON lines, replayed on open |
   | Doublewrite buffer (torn page in a tablespace) | No pages and no doublewrite buffer. A torn tail of the append-only log is discarded |
   | `.ibd` and `innodb_file_per_table=ON` | Not implemented. Every table shares one WAL. `SELECT @@innodb_file_per_table` is errno **1193** (unknown system variable). That is not InnoDB `OFF` (`ibdata1`) either |
   | `innodb_flush_log_at_trx_commit=1` | Default `always`: `fsync` on autocommit and on `COMMIT` |
   | Weaker flush settings | `--wal-sync batch` syncs an explicit `COMMIT` and skips per-statement autocommit sync. `--wal-sync none` does not `fsync`. Neither is the kill-after-commit guarantee |
   | Redo commit record plus undo | An ordinary `COMMIT` is a batch of lines and then one `fsync`. A kill during that append, before `fsync` returns, can replay a prefix of the batch if the operating system kept those lines. After `COMMIT` returns, the `fsync` has finished |
   | Opening a MySQL `.ibd` | Not claimed and not supported |

4. **M210.** [#363](https://github.com/tanbamboo/rusql/issues/363) cannot be marked done unless this ADR is accepted on `main`. Accepting it does not complete M210, M209, or the ultimate MySQL 8.0 goal. The gaps above stay in the compatibility report for that matrix.

## Consequences

- `cargo test -p rusql-storage crash_recovery` commits a row, drops the engine (the kill model: the heap is not on disk and `PersistentEngine` has no `Drop` flush), appends a torn tail, and checks that the committed row is present and the torn bytes are gone.
- A restarted server with the same `--data-dir` shows those rows. It does not create `.ibd` files.
- `information_schema` tablespace names stay errno **1146** (M194).

## Out of scope

- `.ibd`, `ibdata1`, and InnoDB redo file compatibility
- A doublewrite buffer
- `innodb_file_per_table` as a switch that creates files
- `information_schema.INNODB_TABLESPACES`
- Claiming MySQL 8.0 compatibility is complete

## How to verify

```bash
cargo test -p rusql-storage crash_recovery
```

```sql
CREATE TABLE t (id INT PRIMARY KEY);
INSERT INTO t VALUES (1);
-- After this returns, stop the process and start again with the same --data-dir.
SELECT id FROM t;
```

`SELECT @@innodb_file_per_table` is errno **1193**.

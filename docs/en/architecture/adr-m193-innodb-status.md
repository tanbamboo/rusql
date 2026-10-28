# ADR: M193 live SHOW ENGINE INNODB STATUS

**Status**: Accepted (M193 / [#345](https://github.com/tanbamboo/rusql/issues/345))  
**Date**: 2026-10-28

## Context

M131 made `SHOW ENGINE INNODB STATUS` return one row so clients are not told the statement is unsupported. The `Status` cell was a fixed sentence. Operators and some monitors expect that cell to mention the connection that currently holds a transaction, and the locks that transaction holds.

rusql is not Oracle InnoDB. It has explicit transactions (`BEGIN` / `SAVEPOINT` when none is open) and row/gap locks from `SELECT … FOR UPDATE` and conflicting DML. This slice reports that rusql state. It does not invent mutex counters or buffer-pool numbers. It does not change `CONSTITUTION.md`. `information_schema.INNODB_*` stays M194.

## Decision

1. **Columns stay the M131 shape.** `Type`, `Name`, `Status`. `Type` is `InnoDB`. `Name` is empty. Unknown engine names, including `SHOW ENGINE MUSQL STATUS`, stay errno **1286**. `SHOW ENGINES` is unchanged. `SHOW ENGINE … MUTEX` is still not implemented.

2. **`InnoDB` is a compatibility name.** The engine does not switch, and the text is not an Oracle InnoDB monitor dump. Clients that only check that the statement succeeds and that the header looks like the monitor still see:

   ```
   =====================================
   INNODB MONITOR OUTPUT
   =====================================
   ```

3. **`TRANSACTIONS` lists explicit rusql transactions.** `BEGIN` / `START TRANSACTION`, and `SAVEPOINT` when it opens a transaction, register the connection. `COMMIT`, `ROLLBACK`, `COM_RESET_CONNECTION`, and disconnect remove it. Each line is:

   ```
   ---TRANSACTION {trx_id}, ACTIVE
   MySQL thread id {connection_id}
   ```

   `trx_id` starts at 1 on that server listener (one registry per listener). A second begin on the same connection keeps the same id. Autocommit statements are not left in the list after they finish. XA is not listed.

4. **`LOCKS` lists row and gap locks held by those transactions.** A record lock is `RECORD LOCK table {table} trx id {trx_id} lock_mode X pk {pk}`. A gap lock is `GAP LOCK table {table} trx id {trx_id} lock_mode X low {low} high {high}` (`-` when a bound is open). Notes are recorded when the lock is acquired and dropped when that connection's row locks are released or the transaction ends. There is no semaphore section and no mutex counter.

5. **Format tokens stay English in every locale.** `INNODB MONITOR OUTPUT`, `TRANSACTIONS`, `LOCKS`, `---TRANSACTION`, `MySQL thread id`, `RECORD LOCK`, `GAP LOCK`, and `lock_mode X` are the monitor text clients look for. They are not translated errors. Errno **1286** stays in `rusql-i18n`.

6. **No buffer pool and not every InnoDB section.** The text does not contain `BUFFER POOL` or `SEMAPHORES`. File I/O, the log, the insert buffer, and row-operation history are omitted. mysql-diff `show_engine_innodb_status` keeps `compare_output: false` because the blob is not Oracle's dump.

## Consequences

- A connection that has run `BEGIN` and `SELECT … FOR UPDATE` sees its connection id, trx id, and that row lock in `Status`. After `COMMIT` those lines are gone.
- `COM_CHANGE_USER` still does not roll the rusql transaction back. It does release row locks, so the lock lines for that connection disappear while the transaction line remains if the transaction is still open.
- Empty servers still return the header and the two section titles, with no `---TRANSACTION` lines.

## Out of scope

- Buffer pool, page, and mutex statistics
- Parse compatibility with every InnoDB monitor section
- `information_schema.INNODB_TRX` / `INNODB_LOCKS` (M194)
- Claiming MySQL 8.0 compatibility is complete

## How to verify

```bash
cargo test -p rusql-executor innodb_status_live
cargo test -p rusql-server show_engine_innodb_status
```

```sql
BEGIN;
SELECT id FROM t WHERE id = 1 FOR UPDATE;
SHOW ENGINE INNODB STATUS;
COMMIT;
```

While the transaction is open, `Status` includes `MySQL thread id` for that connection and `RECORD LOCK` for the row. After `COMMIT` those lines are gone. `SHOW ENGINE MUSQL STATUS` is errno 1286.

# ADR: M177 Replica failover (manual promote)

**Status**: Accepted (M177 / [#329](https://github.com/tanbamboo/rusql/issues/329))  
**Date**: 2026-10-26

## Context

MySQL 8.0 async replication failover is an operator procedure. It is not Group Replication, InnoDB Cluster, or an external orchestrator. rusql already writes binlog events and can dump them (M71–M175). When this ADR was accepted, `CHANGE MASTER` / `START SLAVE` (M179) and persisted `@@gtid_executed` (M176) were not yet on `main`. Both later landed (M179 PR #432, M176 PR #429).

This ADR is the failover subset implemented for M177. Nothing outside this list is implied.

## Decision

1. **No automatic failover.** rusql never promotes on startup, crash, or dump. There is no orchestrator and no Group Replication (M199).

2. **Writable by default.** A data directory with no `{data_dir}/replica-role.json` accepts writes (primary).

3. **Fence.** `SET GLOBAL super_read_only = ON` sets both `super_read_only` and `read_only` and persists them. `SET GLOBAL read_only = ON` sets only `read_only`.

4. **Promote.** `SET GLOBAL read_only = OFF` clears **both** flags (MySQL coupling) and persists the writable role. That is the promote command. `SET GLOBAL super_read_only = OFF` clears only `super_read_only` and leaves `read_only` set.

5. **Who may write.**
   - `super_read_only` ON: every account, including SUPER (`root`), gets errno **1290** and the `--super-read-only` option text.
   - `read_only` ON and `super_read_only` OFF: non-SUPER accounts get errno **1290** and `--read-only`. SUPER may write.
   - `SELECT` and other read-only statements stay allowed.

6. **Scope errors.** `read_only` and `super_read_only` are GLOBAL only. `SET read_only = …` or `SET SESSION` is errno **1229**. A value other than `0`/`1`/`ON`/`OFF` (and the usual boolean aliases, plus `DEFAULT` = OFF) is errno **1231**.

7. **No silent split-brain.** Promote updates only the data directory of the process that ran the statement. A second directory stays fenced if it was fenced. Restart does not clear or set the flag. rusql does **not** connect to the old primary and does **not** set it read-only. The operator must fence the old primary **before** promoting the replica. Two writable directories are an operator error, not something rusql will paper over.

8. **Manual sequence** (tested pieces are the `SET GLOBAL` statements; slave-thread commands are not in this subset):

   ```sql
   -- On the old primary first (stop application writes):
   SET GLOBAL super_read_only = ON;

   -- On the replica process, after you have stopped applying (there is no SQL thread in this milestone):
   SET GLOBAL read_only = OFF;
   ```

   Compare binlog position out of band (`SHOW MASTER STATUS` file/position, `{data_dir}/gtid.json`). This milestone does not persist `@@gtid_executed` (M176) and does not implement `STOP SLAVE` / `RESET SLAVE` / `CHANGE MASTER` (M179).

## Consequences

- Cloning a promoted data directory clones the writable flag. Treat the copy as a second primary unless you fence it.
- Stored-program statements, account DDL, `LOCK TABLES`, `XA PREPARE`/`XA COMMIT`, and `COMMIT` of pending DML are rejected with errno 1290 while the role blocks that user. This is stricter than MySQL for a `CALL` that would only read.
- Not delayed replication. Not semi-sync (M178). Not InnoDB Cluster.

## Out of scope

- Group Replication and automatic leader election
- `STOP SLAVE` / `RESET SLAVE` / `START REPLICA SQL_THREAD` (M179)
- GTID auto-position and `@@gtid_executed` handshake (M176)

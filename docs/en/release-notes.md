# Release Notes

What landed on `main` and how to verify it. For day-to-day usage see [user-guide.md](user-guide.md).

**中文**: [release-notes.md](../zh-CN/release-notes.md)

---

## Latest: M176 @@gtid_executed persistence (2026-10-21)

**What**: `SELECT @@gtid_executed` / `@@global.gtid_executed` returns the persisted executed GTID set from `{data_dir}/gtid.json` (MySQL-shaped `uuid:interval` text after producing/applying GTIDs on `COMMIT`). A fresh server returns an empty string (not “unsupported”). `@@session.gtid_executed` is rejected (errno 1238, GLOBAL-only). The same text fills `SHOW MASTER STATUS` / `SHOW BINARY LOG STATUS` `Executed_Gtid_Set` and `SHOW GLOBAL VARIABLES LIKE 'gtid_executed'`. Read-only (SET errno 1238). Not `gtid_mode` switching, Group Replication sets, or `mysql.gtid_executed`.

```bash
cargo test -p rusql-storage gtid_executed
cargo test -p rusql-executor gtid_executed
cargo test -p rusql-server gtid_executed
```

## Previous: M175 live SHOW MASTER STATUS (2026-10-17)

**What**: `SHOW MASTER STATUS` and MySQL 8.4 `SHOW BINARY LOG STATUS` return live `File` and `Position` from the open binlog writer (position advances after transaction `COMMIT`). `Binlog_Do_DB` and `Binlog_Ignore_DB` remain empty stubs; `Executed_Gtid_Set` is filled by M176. `SHOW BINARY LOGS` unchanged.

```bash
cargo test -p rusql-storage master_binlog_status
cargo test -p rusql-executor show_master_status
cargo test -p rusql-server show_master_status
```

## Previous: M174 binlog heartbeat events (2026-10-17)

**What**: Idle `COM_BINLOG_DUMP` follow (flags `0`) sends synthetic `HEARTBEAT_LOG_EVENT` (type 27) on a timer so replica IO threads stay connected. Default interval 30s (`RUSQL_MASTER_HEARTBEAT_PERIOD_SECS`); extended dump packets can supply replica `master_heartbeat_period` (nanoseconds, clamped 1–3600s). Heartbeats are not stored in binlog files and are ignored by `apply_binlog_file` / `apply_binlog_bytes`. `BINLOG_DUMP_NON_BLOCK` does not send heartbeats. Not semi-sync (M178) or `COM_BINLOG_DUMP_GTID`.

```bash
cargo test -p rusql-protocol heartbeat
cargo test -p rusql-storage heartbeat
cargo test -p rusql-server binlog_heartbeat
```

## Previous: M173 GTID_LOG_EVENT type 33 (2026-10-17)

**What**: Each transaction `COMMIT` writes `GTID_LOG_EVENT` (type 33) before row or `QUERY_EVENT` payloads. GTIDs come from `{data_dir}/gtid.json` (M58 MVP UUID format). `COM_BINLOG_DUMP` includes type 33; `apply_binlog_file` replays DML without aborting on GTID events; `apply_binlog_bytes` can skip GTIDs already in `GtidState.applied`. `SHOW BINLOG EVENTS` lists `Gtid` with `GTID_NEXT` info text. Not `@@gtid_executed` (M176) or checksum bytes.

```bash
cargo test -p rusql-storage gtid_event
cargo test -p rusql-server gtid_dump
cargo test -p rusql-server binlog_dump
```

## Previous: M172 DELIMITER in COM_QUERY (2026-10-17)

**What**: COM_QUERY accepts client `DELIMITER` lines (no-op OK) and splits scripts on the session delimiter so `CREATE PROCEDURE … BEGIN …; END//` batches work over the wire. Delimiter state resets on connection reset/change user. String literals are not split on `;`. Not server-side mysql CLI; Docker MySQL still expects single-statement `CREATE` in mysql-diff.

```bash
cargo test -p rusql-sql delimiter_stub
cargo test -p rusql-server delimiter_stub
cargo test -p rusql-server run_mysql_test_subset -- --exact run_mysql_test_subset
```

## Previous: M171 trigger timings (2026-10-17)

**What**: All six MySQL row trigger timings (`BEFORE`/`AFTER` × `INSERT`/`UPDATE`/`DELETE`) fire on matching DML. `BEFORE INSERT`/`UPDATE` accept `SET NEW.col`; `BEFORE DELETE` can abort with `SIGNAL`. `AFTER INSERT` runs side-effect DML with `NEW` substitution (same model as existing `AFTER UPDATE`/`DELETE`). `DROP TRIGGER IF EXISTS` parses correctly. Not statement-level triggers or multiple triggers per timing.

```bash
cargo test -p rusql-executor trigger_before_update
cargo test -p rusql-executor trigger_timings
node scripts/mysql-diff.mjs
```

## Previous: M170 condition handlers (2026-10-17)

**What**: `DECLARE CONTINUE HANDLER FOR NOT FOUND SET done = 1` catches cursor end-of-data (errno **1329**) and resumes; `DECLARE EXIT HANDLER FOR SQLEXCEPTION` runs on `SIGNAL` and leaves the block without failing the whole `CALL`. Specific MySQL errno handlers match `SIGNAL`/`FETCH` errors. Unhandled `SIGNAL` still ERRs the client; `RESIGNAL` inside an active handler re-raises the caught condition. Not `UNDO` handlers or `GET DIAGNOSTICS`.

```bash
cargo test -p rusql-sql handler_decl
cargo test -p rusql-executor handler_
cargo test -p rusql-server procedure_handlers
```

## Previous: M169 procedure cursors (2026-10-17)

**What**: `DECLARE cur CURSOR FOR SELECT id FROM t ORDER BY id`, `OPEN cur`, `FETCH cur INTO v`, and `CLOSE cur` run inside `CALL` and assign rows into locals. One open cursor per `CALL` (errno **1326** on second `OPEN`). `FETCH`/`CLOSE` without open cursor errno **1325**; undeclared cursor **1328**; `FETCH` after last row errno **1329** (catch with M170 `NOT FOUND` handlers). Not nested cursors or updatable cursors.

```bash
cargo test -p rusql-sql cursor_decl
cargo test -p rusql-executor cursor_fetch
cargo test -p rusql-server procedure_cursors
```

## Previous: M168 procedure control flow (2026-10-17)

**What**: `IF n > 0 THEN SELECT 1; ELSE SELECT 0; END IF`, `WHILE n > 0 DO SET n = n - 1; END WHILE`, and labeled `LOOP` / `LEAVE` run inside `CALL` (not parse-only). Bad `LEAVE` label errno **1308**. `WHILE`/`LOOP` iteration cap **1_000_000** per `CALL` (documented). Not `REPEAT` / `CASE` statement / `GOTO`.

```bash
cargo test -p rusql-executor proc_if
cargo test -p rusql-executor proc_while
cargo test -p rusql-server procedure_control_flow
```

## Previous: M167 DECLARE variables (2026-10-17)

**What**: `CREATE PROCEDURE gap_decl() BEGIN DECLARE n INT DEFAULT 1; SET n = n + 1; SELECT n; END` then `CALL gap_decl()` returns `2`. Bare identifiers in the body refer to locals; `@name` stays a user variable. `DECLARE` after other statements fails at `CREATE PROCEDURE` with errno **1337** (i18n). Duplicate locals errno **1338**. Not cursors/handlers (M169/M170).

```bash
cargo test -p rusql-sql declare_var
cargo test -p rusql-executor declare_var
cargo test -p rusql-server procedure_declare_var
```

## Previous: M166 SIGNAL / RESIGNAL (2026-10-17)

**What**: `CREATE PROCEDURE p() BEGIN SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'nope', MYSQL_ERRNO = 1644; END` then `CALL p()` returns errno **1644** with message `nope` and SQLSTATE `45000` on the wire. `SIGNAL SQLSTATE '45000'` without `MYSQL_ERRNO` uses default errno **1644** (i18n default message). `RESIGNAL` outside a handler is errno **1645** until M170 condition handlers. Not `GET DIAGNOSTICS` or cursors (M169).

```bash
cargo test -p rusql-sql parse_signal
cargo test -p rusql-executor signal
cargo test -p rusql-server procedure_signal
```

## Previous: M165 OUT / INOUT procedure params (2026-10-17)

**What**: `CREATE PROCEDURE p(OUT x INT) BEGIN SET x = 3; END` then `CALL p(@a)`; `SELECT @a` → `3`. `INOUT` reads the incoming user variable and writes back (`SET x = x + 1`). `CALL` with a literal for `OUT`/`INOUT` is errno **1414** (i18n). `SHOW CREATE PROCEDURE` and `information_schema.PARAMETERS` include `OUT`/`INOUT`. `IN`-only procedures unchanged. Not cursors (M169).

```bash
cargo test -p rusql-sql procedure_out
cargo test -p rusql-executor procedure_out
cargo test -p rusql-server procedure_out_inout
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in. Phases V–Z (including M209/M210) remain.

## Previous: M164 GET_LOCK timeout wait (2026-10-17)

**What**: `GET_LOCK(name, timeout)` with `timeout > 0` blocks up to that many whole seconds (negative timeout waits until the name is free). `timeout 0` stays immediate `1`/`0`. `RELEASE_LOCK` from the holder wakes waiters. `IS_FREE_LOCK` / `IS_USED_LOCK` report lock state. NULL/empty name errno **3057**; name longer than 64 bytes errno **1470** (i18n). Not InnoDB row locks (M157).

```bash
cargo test -p rusql-core user_lock
cargo test -p rusql-executor get_lock_timeout
cargo test -p rusql-server get_lock_timeout
```

**Limits**: Blocking runs inside query execution (same as other sync waits); not fractional-second timeout precision. rusql is still **not** a MySQL 8.0 drop-in. Phases V–Z (including M209/M210) remain.

## Previous: M163 LOCK TABLES / UNLOCK TABLES (2026-10-17)

**What**: Session `LOCK TABLES t READ|WRITE` and `UNLOCK TABLES` for mysqldump-style table locks. **`WRITE`** blocks other sessions until unlock or errno **1205** (i18n). **`READ`** allows concurrent reads, blocks writers. Unknown table errno **1146**; access to an unlocked table in lock mode errno **1100**; write to a `READ`-locked table errno **1099** (i18n). `LOCK TABLES` implicitly commits an open transaction.

```bash
cargo test -p rusql-storage table_locks
cargo test -p rusql-executor table_lock
cargo test -p rusql-server lock_tables
```

**Limits**: Not `READ LOCAL`, global read lock, or views/temp tables. rusql is still **not** a MySQL 8.0 drop-in. Phases U–Z (including M209/M210) remain.

## Previous: M162 XA transactions (2026-10-17)

**What**: Single-branch XA (`XA START 'xid'` … `END` … `PREPARE` … `COMMIT`/`ROLLBACK`). `PREPARE` fsyncs pending DML to additive WAL; restart keeps prepared xids in `XA RECOVER`. Illegal transitions errno **1399** (i18n). Ordinary `BEGIN`/`COMMIT` unchanged.

```bash
cargo test -p rusql-storage xa
cargo test -p rusql-server xa
```

**Limits**: Quoted one-part xid only — not `JOIN`/`RESUME`/`ONE PHASE` or multi-resource XA. rusql is still **not** a MySQL 8.0 drop-in. Phases U–Z (including M209/M210) remain.

## Previous: M161 Gap / next-key locks (2026-10-17)

**What**: ADR [adr-m161-gap-next-key-locks.md](../architecture/adr-m161-gap-next-key-locks.md). Under **REPEATABLE READ** or **SERIALIZABLE**, a PK **range** `SELECT … FOR UPDATE` (e.g. `WHERE id BETWEEN 10 AND 20`) holds a gap lock on that interval. Another connection’s `INSERT` with a PK in the range blocks until commit or errno **1205** (i18n). Gap waits participate in M160 deadlock detection (errno **1213**). **READ COMMITTED** does not take gap locks.

```bash
cargo test -p rusql-storage gap_lock
cargo test -p rusql-server gap_lock
```

**Limits**: Single-column PK, literal `INSERT … VALUES` wait path, `BETWEEN` / simple PK range `AND` only — not full InnoDB next-key matrix, `FOR SHARE` gaps, or predicate SERIALIZABLE extras. rusql is still **not** a MySQL 8.0 drop-in. Phases U–Z (including M209/M210) remain.

## Previous: M160 Deadlock detection (2026-10-17)

**What**: Conflicting row locks (M157) now detect wait-for cycles instead of hanging until `innodb_lock_wait_timeout`. Classic opposite-order `UPDATE` on two rows: one connection gets errno **1213** (i18n) and its transaction is rolled back; the other completes. Victim rule: highest connection id in the cycle (documented MVP).

```bash
cargo test -p rusql-storage deadlock
cargo test -p rusql-server deadlock
```

**Limits**: Not performance_schema or InnoDB-style victim weights. rusql is still **not** a MySQL 8.0 drop-in. Phases U–Z (including M209/M210) remain.

## Previous: M159 SERIALIZABLE isolation (2026-10-17)

**What**: `SET SESSION TRANSACTION ISOLATION LEVEL SERIALIZABLE` (M84 overlay) now changes engine behavior: **REPEATABLE READ** snapshot plus implicit **`FOR UPDATE`** row locks on plain `SELECT` over base tables with a single-column PRIMARY KEY (documented MVP — not InnoDB next-key/gap locks; see M161). A concurrent `UPDATE` on a locked row blocks like explicit `FOR UPDATE` (M157). A second `SELECT` in the same transaction does not show phantoms from commits after the first scan (RR snapshot). **READ COMMITTED** and default RR are unchanged.

```bash
cargo test -p rusql-storage serializable
cargo test -p rusql-server serializable
```

**Limits**: Inserts into ranges with no locked rows are not gap-blocked; JOIN/derived `SELECT` does not implicit-lock yet. rusql is still **not** a MySQL 8.0 drop-in. Phases U–Z (including M209/M210) remain.

## Previous: M158 READ COMMITTED vs snapshot (2026-10-17)

**What**: `SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED` (M84 overlay) now changes engine behavior in explicit transactions: each statement sees rows committed by other connections since the prior statement. Default **REPEATABLE READ** keeps the M32 snapshot for the whole transaction (second `SELECT` in the same `BEGIN` does not see concurrent commits). `READ UNCOMMITTED` does not enable dirty reads.

```bash
cargo test -p rusql-storage read_committed
cargo test -p rusql-server read_committed
cargo test -p rusql-server snapshot_isolation
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in. Phases U–Z (including M209/M210) remain.

## Previous: M157 SELECT … FOR UPDATE waits (2026-10-17)

**What**: On tables with a single-column PRIMARY KEY, `SELECT … FOR UPDATE` in a transaction holds exclusive row locks until `COMMIT`/`ROLLBACK`. Another connection’s `UPDATE` on the same row blocks until release or `innodb_lock_wait_timeout` (default 50 seconds; `SET SESSION innodb_lock_wait_timeout`) with errno 1205 (i18n). M85 lock-clause syntax remains accepted; `FOR SHARE` is not row-locking yet; `NOWAIT`/`SKIP LOCKED` do not fail-fast or skip (out of M157 scope). Not gap locks (M161) or tables without a single-column PK.

```bash
cargo test -p rusql-storage for_update_wait
cargo test -p rusql-server for_update_wait
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in. Phases U–Z (including M209/M210) remain.

## Previous: M156 information_schema.COLUMNS extras (2026-10-16)

**What**: Virtual `information_schema.COLUMNS` now fills `COLUMN_DEFAULT`, `EXTRA`, and `COLUMN_KEY` (after the existing M12 columns). Primary keys and `AUTO_INCREMENT` show `COLUMN_KEY=PRI` and `EXTRA` containing `auto_increment`; VIRTUAL generated columns (M146) show `VIRTUAL GENERATED` in `EXTRA`. Secondary and unique indexes map to `MUL` / `UNI`; foreign-key columns to `MUL`. Querying a non-existent `information_schema` table still returns errno 1146; filtering to an unknown schema returns empty rows. mysql-diff suite `information_schema_columns_extra`.

```bash
cargo test -p rusql-executor information_schema_columns
cargo test -p rusql-server columns_extra
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in. Phase T exit (simple mysqldump load) and Phases U–Z (including M209/M210) remain.

## Previous: M155 RENAME TABLE multi-pair (2026-10-16)

**What**: `RENAME TABLE t1 TO t1_old, t2 TO t1` renames multiple pairs in one statement (atomic swap). Unknown source is errno 1146; conflicting target is errno 1050; no partial apply on error. Parsed in `rusql-sql` and executed via storage `rename_tables`. Single-table `ALTER TABLE … RENAME TO` unchanged. mysql-diff suite `rename_table_multi`.

```bash
cargo test -p rusql-storage rename_table
cargo test -p rusql-executor rename_table
cargo test -p rusql-server rename_table_multi_pair
```

## Previous: M154 CREATE TABLE LIKE / AS SELECT (2026-10-16)

**What**: `CREATE TABLE t2 LIKE t1` copies column definitions (PRIMARY KEY, defaults, CHECK, VIRTUAL generated) into an empty table; FOREIGN KEY, RANGE partitions, and secondary indexes are not copied. `CREATE TABLE t3 AS SELECT id, n FROM t1` infers types from the result and inserts rows. Unknown `LIKE` source is errno 1146. `CREATE TABLE IF NOT EXISTS` is a no-op when the target exists. Not `LIKE` on views or `AS SELECT *`. mysql-diff suite `create_table_like_ctas`.

```bash
cargo test -p rusql-executor create_table_like
cargo test -p rusql-executor create_table_as_select
cargo test -p rusql-server create_table_like_ctas
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in. Phase T continues after M154; Phases T–Z (including M209/M210) remain.

## Previous: M153 ALTER TABLE index DDL (2026-10-16)

**What**: `ALTER TABLE t ADD INDEX idx_n (n)`, `ALTER TABLE t DROP INDEX idx_n`, `DROP INDEX idx_n ON t`, and `ALTER TABLE t ADD UNIQUE (n)` work for mysqldump reloads. Unique indexes from `ADD UNIQUE` / `CREATE UNIQUE INDEX` reject duplicate rows on `INSERT`/`UPDATE` (errno 1062). WAL records `drop_index`. Dump-only `DROP INDEX` spellings rewrite in `rusql-sql`. Not `ALGORITHM`/`LOCK` or fulltext. mysql-diff suite `alter_table_index`.

```bash
cargo test -p rusql-executor alter_index
cargo test -p rusql-sql alter_index
```

## Previous: M152 RANGE table partitioning (2026-10-16)

**What**: `CREATE TABLE t (id INT PRIMARY KEY) PARTITION BY RANGE (id) (PARTITION p0 VALUES LESS THAN (10), PARTITION p1 VALUES LESS THAN MAXVALUE)` creates catalog + per-partition heaps. `INSERT` routes rows; `SELECT … WHERE id = 5` reads partition `p0` only; EXPLAIN `Extra` shows `partitions: p0`. HASH/LIST/KEY rejected (i18n). ADR: [adr-m152-range-partitioning.md](architecture/adr-m152-range-partitioning.md). mysql-diff suite `partition_range`.

```bash
cargo test -p rusql-storage partition_range
cargo test -p rusql-executor partition_range
cargo test -p rusql-server partition_range
```

## Previous: M151 Functional indexes (2026-10-16)

**What**: `CREATE TABLE t (id INT PRIMARY KEY, n INT, INDEX ((n+1)))` plus `INSERT` and `SELECT * FROM t WHERE n + 1 = 2` uses the functional index (EXPLAIN `type=ref`). `CREATE INDEX … ((expr))` accepts the same expression subset (`+` `-` `*` `/`, columns, literals); unknown functions error at DDL (i18n). Not descending or prefix functional indexes. mysql-diff suite `functional_index`.

```bash
cargo test -p rusql-storage functional_index
cargo test -p rusql-planner functional_index
cargo test -p rusql-server functional_index
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in. Phase T continues after M151; Phases T–Z (including M209/M210) remain.

## Previous: M150 Invisible columns / indexes (2026-10-16)

**What**: `CREATE TABLE t (id INT PRIMARY KEY, n INT INVISIBLE)` omits `n` from `SELECT *`; `SELECT n` still works. `INSERT` must supply invisible columns without a default (errno 1364, i18n). `SHOW CREATE TABLE` includes `INVISIBLE`. `CREATE INDEX idx ON t (n) INVISIBLE` marks the index invisible; EXPLAIN and index scans skip it. Not `ALTER … SET INVISIBLE`. mysql-diff suite `invisible_column`.

```bash
cargo test -p rusql-executor invisible_column
cargo test -p rusql-server invisible_column
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in. Phase T continues after M150; Phases T–Z (including M209/M210) remain.

## Previous: M149 DEFAULT expressions (2026-10-16)

**What**: `CREATE TABLE t (id INT PRIMARY KEY, n INT DEFAULT (1+2), ts DATETIME DEFAULT CURRENT_TIMESTAMP)` stores defaults in catalog/WAL. `INSERT` that omits `n` stores `3`; `CURRENT_TIMESTAMP` defaults produce a datetime string via `NOW()`. `UPDATE t SET n = DEFAULT` restores the catalog default. Invalid `DEFAULT (expr)` at `CREATE TABLE` errors (i18n). `SHOW CREATE TABLE` / `DESCRIBE` show defaults. Not `ALTER … SET DEFAULT` or defaults on VIRTUAL generated columns. mysql-diff suite `default_expr`.

```bash
cargo test -p rusql-executor default_expr
cargo test -p rusql-sql default_expr
cargo test -p rusql-server default_expr
```

## Previous: M148 ENUM / SET types (2026-10-16)

**What**: `CREATE TABLE t (id INT PRIMARY KEY, e ENUM('a','b'), s SET('a','b','c'))` stores member strings in heap rows. Valid `INSERT`/`UPDATE` succeed; invalid members with default strict `@@sql_mode` are errno 1265 (i18n). `SET` values are canonicalized to declaration order (`'b,a'` → `'a,b'`). `SHOW CREATE TABLE` lists `ENUM`/`SET` members. Not numeric ENUM index literals or `FIND_IN_SET`. mysql-diff suite `enum_set_types`.

```bash
cargo test -p rusql-executor enum_type
cargo test -p rusql-executor set_type
cargo test -p rusql-server enum_set_types
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in. Phase T continues after M148; Phases T–Z (including M209/M210) remain.

## Previous: M147 CHECK constraints (2026-10-16)

**What**: `CREATE TABLE t (id INT PRIMARY KEY, n INT CHECK (n > 0))` and `CONSTRAINT c CHECK (expr)` persist check expressions in catalog/WAL. `INSERT`/`UPDATE` evaluate checks per row; violations are errno 3819 (i18n). `SHOW CREATE TABLE` includes `CHECK` clauses. Checks are not evaluated on `SELECT`. `information_schema.TABLE_CONSTRAINTS` CHECK rows are unchanged. Not `CHECK NOT ENFORCED`, `ALTER TABLE … ADD CHECK`, or full MySQL check naming parity. mysql-diff suite `check_constraint`.

```bash
cargo test -p rusql-executor check_constraint
cargo test -p rusql-sql check_constraint
cargo test -p rusql-server check_constraint
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in. Phase T continues after M147; Phases T–Z (including M209/M210) remain.

## Previous: M146 VIRTUAL generated columns (2026-10-16)

**What**: `CREATE TABLE t (id INT PRIMARY KEY, n INT, g INT AS (n + 1) VIRTUAL)` stores the generation expression in catalog/WAL; `SELECT g` evaluates `n + 1` on read. `SHOW CREATE TABLE` includes `AS (n + 1) VIRTUAL` (spacing may differ from MySQL). Explicit `INSERT`/`UPDATE` targeting `g` is errno 3105 (i18n). `INSERT (id, n)` omits `g` and still returns computed values. `STORED` / `PERSISTENT` generated columns error (i18n). Not STORED generated columns, VIRTUAL column indexes (M151), or every MySQL generation function. mysql-diff suite `generated_column_virtual`.

```bash
cargo test -p rusql-executor generated_column
cargo test -p rusql-storage generated_column
cargo test -p rusql-server generated_column_virtual
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in. Phase T started with M146; Phases T–Z (including M209/M210) remain after that merge.

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

## Previous: M145 EXPLAIN FORMAT=JSON stub (2026-10-15)

**What**: `EXPLAIN FORMAT=JSON SELECT …` returns one row and one column (`EXPLAIN`) whose cell is JSON with a documented stub shape: required top-level `query_block`, nested `table` object with `table_name` and `access_type` (same access path as M49 text `EXPLAIN`), optional `key`, and `rows_examined_per_scan`. Plain `EXPLAIN` text output is unchanged. `EXPLAIN ANALYZE` still errors (not a silent wrong plan). Not full MySQL optimizer JSON or live cost numbers (M49/M205). mysql-diff suite `explain_json` uses `compare_output: false`.

```bash
cargo test -p rusql-executor explain_json
cargo test -p rusql-server explain_json
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in. Phase S (M133–M145) is complete on `main` after this merge; Phase T–Z (including M209/M210) remain.

## Previous: M144 multi-table UPDATE / DELETE (2026-10-14)

**What**: `UPDATE t1 INNER JOIN t2 ON t1.id = t2.id SET t1.n = t2.n` copies matching values into `t1`; `DELETE t1 FROM t1 INNER JOIN t2 ON t1.id = t2.id` removes matching rows from `t1` only. Unknown alias/column errors; single-table DML unchanged. Single-column PK on the target table. Not multi-target delete or outer-join DML. mysql-diff suite `multi_table_dml`.

```bash
cargo test -p rusql-executor multi_update
cargo test -p rusql-executor multi_delete
cargo test -p rusql-server multi_table_dml
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in.

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

## Previous: M143 INSERT … SET (2026-10-13)

**What**: `INSERT INTO t SET id=1, n=2` inserts the same row as `INSERT INTO t (id, n) VALUES (1, 2)`. Unknown column errors match column-list `INSERT`; duplicate PRIMARY KEY is errno 1062. `INSERT IGNORE … SET` skips PK conflicts. M68 `INSERT … SELECT` / `ON DUPLICATE KEY UPDATE` and M111 `REPLACE INTO` are unchanged. Not multi-table INSERT. mysql-diff suite `insert_set`.

```bash
cargo test -p rusql-sql insert_set
cargo test -p rusql-executor insert_set
cargo test -p rusql-server insert_set
```

**Limits**: rusql is still **not** a MySQL 8.0 drop-in.

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

## Previous: M142 GREATEST / LEAST (2026-10-13)

**What**: `SELECT GREATEST(1, 3, 2)` returns `3`; `SELECT LEAST(1, 3, 2)` returns `1`. Any NULL argument yields SQL NULL (`SELECT GREATEST(1, NULL)`). Fewer than two arguments is errno 1582. Mixed quoted-string/numeric literals compare as strings (`GREATEST('10', 2)` → `2`; `LEAST('2', 10)` → `10`). Not window functions. mysql-diff suite `greatest_least`.

```bash
cargo test -p rusql-executor greatest_least
cargo test -p rusql-server greatest_least
```

**Limits**: NULL propagation follows default `sql_mode` only (not `HIGH_NOT_PRECEDENCE` / `IGNORE`). rusql is still **not** a MySQL 8.0 drop-in.

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

## Previous: M141 String pack (2026-10-13)

**What**: `SELECT TRIM('  a  ')` returns `a`. `SELECT TRIM(LEADING 'x' FROM 'xxa')` returns `a`. `SELECT REPLACE('abc', 'b', 'x')` returns `axc`; empty search string returns the original string. `SELECT SUBSTRING_INDEX('a.b.c', '.', 2)` returns `a.b`; negative count returns segments from the right (`b.c` for `-2`). M113 `SUBSTRING`/`ROUND`/`DATE_ADD` unchanged. Not `REGEXP`/`RLIKE`. mysql-diff suite `string_pack`.

```bash
cargo test -p rusql-executor trim
cargo test -p rusql-executor replace_fn
cargo test -p rusql-executor substring_index
cargo test -p rusql-server string_pack
```

## Previous: M140 Date pack (2026-10-13)

**What**: `SELECT DATE_SUB('2026-01-02', INTERVAL 1 DAY)` returns `2026-01-01` (same INTERVAL units and date-vs-datetime output rules as M113 `DATE_ADD`/`ADDDATE`). `SELECT DATEDIFF('2026-01-02', '2026-01-01')` returns `1` (date-only difference; time portions ignored). `SELECT DATE_FORMAT('2026-01-02', '%Y-%m-%d')` returns `2026-01-02`; `%Y-%m-%d %H:%i:%s` works on datetime inputs. Only `%Y`, `%m`, `%d`, `%H`, `%i`, `%s`, and `%%` are supported — other specifiers error. Not `STR_TO_DATE` or full MySQL format vocabulary. mysql-diff suite `date_pack`.

```bash
cargo test -p rusql-executor date_sub
cargo test -p rusql-executor datediff
cargo test -p rusql-executor date_format
cargo test -p rusql-server date_pack
```

**Limits**: `MONTH`/`YEAR` intervals still use the M105 30/365-day approximation. rusql is still **not** a MySQL 8.0 drop-in.

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

## Previous: M139 CAST / CONVERT charset (2026-10-12)

**What**: `SELECT CONVERT('abc' USING utf8mb4)` returns `abc`. `CONVERT('abc', CHAR CHARACTER SET utf8mb4)` is equivalent for this slice (identity when bytes are already utf8mb4). Unknown charset is errno 1115 with an i18n message (same family as M114). Existing `CAST(x AS CHAR)` and numeric `CAST` are unchanged. Not `CONVERT(expr, DATETIME)`-style type conversion, latin1 transcoding, or `utf8` alias unless added later. mysql-diff suite `convert_charset`.

```bash
cargo test -p rusql-executor convert_charset
cargo test -p rusql-server convert_charset
```

**Limits**: Only `utf8mb4` is accepted for charset conversion; real transcoding from other encodings is not implemented. rusql is still **not** a MySQL 8.0 drop-in.

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

## Previous: M138 VALUES row constructor (2026-10-12)

**What**: MySQL table-value constructor `VALUES ROW(1), ROW(2)` works as a top-level query or as a derived table in `FROM`. Default column names are `column_0`, `column_1`, … A mismatch in expression count across `ROW()` lists is errno 1136 with an i18n message. `INSERT … VALUES` row lists are unchanged. Not `TABLE t` syntactic sugar. mysql-diff suite `values_row`.

```bash
cargo test -p rusql-sql values_row
cargo test -p rusql-executor values_row
cargo test -p rusql-server values_row_constructor
```

**Limits**: `SELECT * FROM (VALUES (1),(2)) t(n)` without `ROW` and `TABLE t` sugar stay out of scope unless added later.

## Previous: M137 FULL OUTER JOIN (2026-10-12)

**What**: Two-table `FULL OUTER JOIN` / `FULL JOIN` on an equality `ON` key returns matched rows plus NULL-padded unmatched rows from both sides (LEFT JOIN plus unmatched RIGHT). `FULL JOIN` is accepted as a synonym. `INNER` / `LEFT` / `RIGHT` joins are unchanged. `ON` is required. `USING` and natural join stay unsupported. Docker MySQL 8.0 does not parse `FULL OUTER JOIN`; rusql implements the specified equivalent row set. mysql-diff suite `full_outer_join`.

```bash
cargo test -p rusql-sql full_outer_join
cargo test -p rusql-executor full_outer_join
cargo test -p rusql-server full_outer_join
```

**Limits**: Three-or-more-table `FULL JOIN`, `USING`, and natural join are out of scope. rusql is still **not** a MySQL 8.0 drop-in.

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

## Previous: M136 EXCEPT / EXCEPT ALL (2026-10-12)

**What**: `SELECT 1 EXCEPT SELECT 1` returns zero rows. `SELECT 1 EXCEPT SELECT 2` returns `1`. `EXCEPT` is distinct (a value that appears on the right is removed entirely from the left). `EXCEPT ALL` keeps leftover left duplicates: two left rows of `1` minus one right row of `1` returns one row `1`. Column count mismatch is errno 1222 with an i18n message. `UNION` / `UNION ALL` / `INTERSECT` are unchanged. Not `INTERSECT ALL` or `MINUS`. mysql-diff suite `except`.

```bash
cargo test -p rusql-sql except
cargo test -p rusql-executor except
cargo test -p rusql-server except
```

**Limits**: `INTERSECT ALL` and the `MINUS` synonym stay out of scope. rusql is still **not** a MySQL 8.0 drop-in.

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

## Previous: M135 LAG / LEAD / SUM() OVER (2026-10-12)

**What**: `LAG(col)` / `LAG(col, 1)` and `LEAD(col)` with `OVER (ORDER BY id)` match MySQL on a 3-row fixture (first `LAG` is NULL, last `LEAD` is NULL). Offset defaults to 1; an optional third argument is the out-of-range default. `SUM(n) OVER (PARTITION BY g ORDER BY id)` is a running sum per partition without a `ROWS` clause. With `ORDER BY` and no frame the default is `UNBOUNDED PRECEDING` … `CURRENT ROW`; without `ORDER BY` the sum is the whole partition. M129 `ROWS BETWEEN` frames apply to `SUM`. Wrong arity is errno 1582 (i18n). Existing ranking windows are unchanged. Not `NTH_VALUE` / `CUME_DIST` / `RANGE` / named windows. mysql-diff suite `lag_lead_sum_over`.

```bash
cargo test -p rusql-executor lag_lead
cargo test -p rusql-executor sum_over
cargo test -p rusql-server lag_lead
```

**Limits**: `NTH_VALUE`, `CUME_DIST`, `RANGE BETWEEN`, and named `WINDOW` clauses are out of scope (M203). rusql is still **not** a MySQL 8.0 drop-in.

## Previous: M134 JSON_OBJECT / JSON_ARRAY / JSON_SET (2026-10-11)

**What**: `SELECT JSON_OBJECT('a', 1, 'b', 'x')` returns `{"a": 1, "b": "x"}`. `SELECT JSON_ARRAY(1, 'x')` returns `[1, "x"]`. `SELECT JSON_SET('{"a":1}', '$.b', 2)` adds `b`; `JSON_SET` on an existing key replaces it. A missing last-component key is added when the parent object exists (`JSON_SET('{}', '$.a', 1)` → `{"a": 1}`). Nested paths whose parent is missing are a no-op like MySQL 8.0 (`JSON_SET('{}', '$.a.b', 1)` → `{}`). Odd-length `JSON_OBJECT` args and incomplete `JSON_SET` pairs are errno 1582. NULL object keys are errno 3158. Invalid JSON documents are errno 3141 (i18n). Paths are the M115 `$.key` / `$.a.b` subset. Not `JSON_REPLACE` / `JSON_REMOVE` / `JSON_TABLE` or full JSONPath. mysql-diff suite `json_object`.

```bash
cargo test -p rusql-executor json_object
cargo test -p rusql-server json_set
```

**Limits**: Array JSONPath (`$[0]`), `JSON_REPLACE` / `JSON_REMOVE` / `JSON_INSERT`, and binary JSON storage beyond M40 text/JSON cells are out of scope. rusql is still **not** a MySQL 8.0 drop-in.

## Previous: M133 JSON_UNQUOTE / -> / ->> (2026-10-11)

**What**: `SELECT JSON_UNQUOTE(JSON_EXTRACT('{"a":"x"}', '$.a'))` returns unquoted `x`. `col->'$.a'` matches `JSON_EXTRACT` (JSON strings stay quoted). `col->>'$.a'` matches `JSON_UNQUOTE(JSON_EXTRACT(...))`. Missing path is SQL NULL. Invalid JSON is errno 3141 (i18n). `JSON_UNQUOTE` of non-quoted text returns the text as-is. M115 `JSON_EXTRACT` is unchanged. Not `JSON_OBJECT` / `JSON_SET` (M134) or full JSONPath. mysql-diff suite `json_unquote`.

```bash
cargo test -p rusql-sql json_unquote
cargo test -p rusql-executor json_unquote
cargo test -p rusql-server json_unquote
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M132 procedure IN parameters (2026-10-10)

**What**: `CREATE PROCEDURE gap_p(IN x INT) BEGIN SELECT x; END` persists the `IN` parameter. `CALL gap_p(3)` returns one column `3`. `SHOW CREATE PROCEDURE gap_p` includes `IN x INT`. Zero-arg procedures still work. Wrong `CALL` arity is errno 1318. `information_schema.PARAMETERS` lists the persisted `IN` row. Not `OUT`/`INOUT`, `SIGNAL`, or mysql CLI `DELIMITER`. mysql-diff suite `procedure_in_param` uses a single-statement `CREATE` so Docker MySQL accepts it without `DELIMITER`.

```bash
cargo test -p rusql-sql procedure_in
cargo test -p rusql-executor procedure_in
cargo test -p rusql-server procedure_in
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M131 SHOW ENGINE INNODB STATUS stub (2026-10-10)

**What**: `SHOW ENGINE INNODB STATUS` returns one row with MySQL-shaped columns `Type`, `Name`, and `Status`. `Type` is `InnoDB`, `Name` is empty, and `Status` is a documented i18n stub (not live mutex/lock statistics — M193). Unknown engine names are errno 1286. `SHOW ENGINES` is unchanged. mysql-diff suite `show_engine_innodb_status` uses `compare_output: false`.

```bash
cargo test -p rusql-sql engine_innodb
cargo test -p rusql-executor engine_innodb
cargo test -p rusql-server engine_innodb
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M130 CREATE EVENT DISABLE ON SLAVE (2026-10-10)

**What**: `CREATE EVENT e_dos ON SCHEDULE EVERY 1 HOUR DISABLE ON SLAVE DO SELECT 1` persists. `SHOW CREATE EVENT e_dos` reconstructs `DISABLE ON SLAVE`. The scheduler does not run slave-disabled events (same skip as `DISABLE`); `SHOW EVENTS` still lists them with `Status` `SLAVESIDE_DISABLED`. rusql has no replica role yet — this is skip-execute, not `server_id` gating. `ENABLE` / `DISABLE` without `ON SLAVE` are unchanged. mysql-diff suite `event_disable_on_slave`.

```bash
cargo test -p rusql-sql disable_on_slave
cargo test -p rusql-core disable_on_slave
cargo test -p rusql-executor disable_on_slave
cargo test -p rusql-server disable_on_slave
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M129 Window ROWS BETWEEN frames (2026-10-10)

**What**: `ROW_NUMBER()` / `RANK()` / `DENSE_RANK()` accept `OVER (… ROWS BETWEEN …)` with `UNBOUNDED PRECEDING`, `CURRENT ROW`, `n PRECEDING`, and `n FOLLOWING`. Ranking functions ignore the peer set like MySQL 8.0, so framed ranks match unframed ranks. Illegal start/end pairs are errno 3585. `RANGE` / `GROUPS` frames and named `WINDOW` clauses still error. Not aggregate `SUM() OVER`. mysql-diff suite `window_frame_rows`.

```bash
cargo test -p rusql-sql window_frame
cargo test -p rusql-executor window_frame
cargo test -p rusql-server window_frame
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M128 INTERSECT (2026-10-09)

**What**: `SELECT 1 AS n INTERSECT SELECT 1 AS n` returns one row `1`. `SELECT 1 AS n INTERSECT SELECT 2 AS n` returns zero rows. INTERSECT is distinct (duplicate matches appear once). Column count mismatch is errno 1222 with an i18n message. `UNION` / `UNION ALL` are unchanged. Not `INTERSECT ALL` or `EXCEPT`. mysql-diff suite `intersect`.

```bash
cargo test -p rusql-sql intersect
cargo test -p rusql-executor intersect
cargo test -p rusql-server intersect
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M127 WITH RECURSIVE (2026-10-09)

**What**: `WITH RECURSIVE cte AS (SELECT 1 AS n UNION ALL SELECT n + 1 FROM cte WHERE n < 3) SELECT n FROM cte` returns `1,2,3` in generation order (anchor, then each recursive step). Recursion that does not terminate within the documented `cte_max_recursion_depth` cap of 1000 is errno 3636 with an i18n message (not a hang). Non-recursive `WITH` (M69) is unchanged. Not `SEARCH` / `CYCLE`, cycle detection beyond the cap, or recursive DML. mysql-diff suite `with_recursive`.

```bash
cargo test -p rusql-executor recursive
cargo test -p rusql-server recursive
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M126 SAVEPOINT / ROLLBACK TO / RELEASE SAVEPOINT (2026-10-09)

**What**: `SAVEPOINT name` / `ROLLBACK TO SAVEPOINT name` / `RELEASE SAVEPOINT name` are named savepoints inside a transaction. After further DML, `ROLLBACK TO SAVEPOINT` undoes those changes, keeps the transaction open, and leaves the savepoint row from before. `RELEASE SAVEPOINT` then `ROLLBACK TO SAVEPOINT` is errno 1305. `SAVEPOINT` with autocommit (no `BEGIN`) starts a transaction. Savepoints do not survive `COMMIT` / full `ROLLBACK`. Not XA. mysql-diff suite `savepoint` batches SAVEPOINT+ROLLBACK TO on one CLI connection.

```bash
cargo test -p rusql-sql savepoint
cargo test -p rusql-storage savepoint
cargo test -p rusql-executor savepoint
cargo test -p rusql-server savepoint
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M125 text PREPARE / EXECUTE / DEALLOCATE PREPARE (2026-10-08)

**What**: `PREPARE name FROM 'sql'` / `EXECUTE name` / `DEALLOCATE PREPARE name` (and `DROP PREPARE`) are session-scoped named statements for `COM_QUERY` clients. `EXECUTE` matches running the stored SQL. Unknown names are errno 1243. Invalid SQL fails at `PREPARE` (errno 1064). SQL that parses but rusql cannot execute (for example window `RANGE` frames) fails at `EXECUTE`. `COM_RESET_CONNECTION` / `COM_CHANGE_USER` clear the map. Binary `COM_STMT_*` is unchanged. Not `EXECUTE … USING` / `PREPARE … FROM @var`. mysql-diff suite `prepare_execute_text` batches PREPARE+EXECUTE on one CLI connection.

```bash
cargo test -p rusql-sql prepare
cargo test -p rusql-executor prepare
cargo test -p rusql-server prepare
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M124 CREATE OR REPLACE VIEW (2026-10-08)

**What**: `CREATE OR REPLACE VIEW name AS SELECT …` creates the view if it is missing and replaces the stored SELECT when the view already exists. `SHOW CREATE VIEW` and querying the view use the new SQL. Replacing a **base table** of the same name is errno 1347. Plain `CREATE VIEW` still errors on duplicate (M33). Not `ALTER VIEW`, ALGORITHM / DEFINER / SQL SECURITY, or materialized views. mysql-diff suite `create_or_replace_view` batches view DDL with SELECT on one CLI connection (views are session catalog, same as M33); `SHOW CREATE VIEW` uses `compare_output: false` because rusql omits ALGORITHM/DEFINER.

```bash
cargo test -p rusql-executor or_replace_view
cargo test -p rusql-server or_replace_view
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M123 SHOW BINLOG EVENTS (2026-09-30)

**What**: `SHOW BINLOG EVENTS` (optional `IN 'log_name'`, `FROM pos`, `LIMIT`) lists real events from known `{data_dir}/binlog/binlog.NNNNNN` files with columns `Log_name`, `Pos`, `Event_type`, `Server_id`, `End_log_pos`, `Info`. With binlog enabled the first row is the on-disk `Format_desc` event. Unknown types are `Unknown` with an empty Info cell. A missing binlog directory is a documented empty list. Unknown `IN` files are errno 1220. `SHOW BINARY LOGS` is unchanged. Not mysqlbinlog tool compatibility. mysql-diff suite `show_binlog_events` uses `compare_output: false` because payloads differ from Docker MySQL.

```bash
cargo test -p rusql-sql binlog_events
cargo test -p rusql-storage binlog_events
cargo test -p rusql-executor binlog_events
cargo test -p rusql-server binlog_events
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M122 SHOW BINARY LOGS (2026-09-30)

**What**: `SHOW BINARY LOGS` (and the MySQL synonym `SHOW MASTER LOGS`) lists known binlog files with columns `Log_name` and `File_size`. With binlog enabled (server `{data_dir}/binlog/`) the current file is present (`binlog.000001` after open). Sizes are the on-disk length. A missing binlog directory is a documented empty list. Not mysqlbinlog tool compatibility. mysql-diff suite `show_binary_logs` uses `compare_output: false` because names/sizes differ from Docker MySQL.

```bash
cargo test -p rusql-sql binary_log
cargo test -p rusql-executor binary_log
cargo test -p rusql-server binary_log
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M121 information_schema.PARAMETERS (2026-09-30)

**What**: `SELECT SPECIFIC_NAME FROM information_schema.PARAMETERS LIMIT 1` is accepted (not errno 1146). Documented columns include `SPECIFIC_NAME`, `PARAMETER_MODE`, `PARAMETER_NAME`, `DATA_TYPE` (plus `SPECIFIC_SCHEMA`, `ORDINAL_POSITION`, `ROUTINE_TYPE`). Rows are catalogued stored-program parameters. `CREATE PROCEDURE` / `CREATE FUNCTION` still do not persist `IN`/`OUT` lists (M132), so the result is empty and rusql does not invent parameters. `SHOW CREATE PROCEDURE` keeps the empty `()` list. Unknown `information_schema` tables stay errno 1146. mysql-diff suite `information_schema_parameters` uses `compare_output: false` because Docker MySQL lists `sys` parameters.

```bash
cargo test -p rusql-executor parameters
cargo test -p rusql-server parameters
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M120 information_schema.PROCESSLIST (2026-09-29)

**What**: `SELECT ID FROM information_schema.PROCESSLIST` returns live connection rows from the M53 registry (at least this session’s `CONNECTION_ID()`), not errno 1146. I_S column names are `ID`, `USER`, `HOST`, `DB`, `COMMAND`, `TIME`, `STATE`, `INFO`. `SHOW PROCESSLIST` still uses M53 mixed-case names (`Id`, `User`, `Host`, `db`, `Command`, `Time`, `State`, `Info`) and the same order. `COM_PROCESS_INFO` is unchanged. mysql-diff suite `information_schema_processlist` uses `compare_output: false` because Time/Info/ids differ from Docker MySQL.

```bash
cargo test -p rusql-executor processlist
cargo test -p rusql-server processlist
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M119 information_schema.TABLE_CONSTRAINTS (2026-09-29)

**What**: `SELECT CONSTRAINT_NAME FROM information_schema.TABLE_CONSTRAINTS WHERE TABLE_NAME = 'gap_tc'` returns the PRIMARY KEY row (MySQL name `PRIMARY`). UNIQUE column/table constraints and `CREATE UNIQUE INDEX` appear with `CONSTRAINT_TYPE` `UNIQUE`. FOREIGN KEY catalog rows appear as `FOREIGN KEY`. Portable columns: `CONSTRAINT_SCHEMA`, `CONSTRAINT_NAME`, `TABLE_SCHEMA`, `TABLE_NAME`, `CONSTRAINT_TYPE`. CHECK is not emitted (M147). Unknown `information_schema` tables stay errno 1146. `KEY_COLUMN_USAGE` and `EVENTS` are unchanged.

```bash
cargo test -p rusql-executor table_constraints
cargo test -p rusql-server table_constraints
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M118 GET_LOCK / RELEASE_LOCK (2026-09-21)

**What**: `SELECT GET_LOCK('gap_lock', 0)` returns `1` when the name is free. A second connection's `GET_LOCK('gap_lock', 0)` returns `0` while the first holds it. `SELECT RELEASE_LOCK('gap_lock')` returns `1` on the holder (and frees the name), `0` if another session holds it, and NULL if the name is not locked. Re-`GET_LOCK` of a name the same session already holds returns `1`. Disconnect, `COM_RESET_CONNECTION`, and `COM_CHANGE_USER` release that session's locks. A NULL or empty name is errno 3057. Names longer than 64 bytes are errno 1470. Blocking wait for `timeout > 0` is **M164**. M117 `LAST_INSERT_ID(expr)`, M116 `UUID()`, and M115 `JSON_EXTRACT` are unchanged.

```bash
cargo test -p rusql-executor get_lock
cargo test -p rusql-server get_lock
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## M117 LAST_INSERT_ID(expr) (2026-09-21)

**What**: `SELECT LAST_INSERT_ID(5)` returns `5` and sets the connection's session value so a following `SELECT LAST_INSERT_ID()` also returns `5`. Non-integers coerce like MySQL unsigned conversion on portable cases: `LAST_INSERT_ID(5.9)` is `6` (nearest integer, half away from zero); non-numeric strings are `0`; negatives wrap as `BIGINT UNSIGNED`. Other connections do not see the value. `COM_RESET_CONNECTION` / `COM_CHANGE_USER` clear it (same as M75). Generated `AUTO_INCREMENT` INSERT ids still overwrite the session value; AUTO_INCREMENT allocation is unchanged. The setter writes `session.last_insert_id`, so later DML OK packets report that value until a generated INSERT overwrites it. Extra arguments error (i18n). This is not `GET_LOCK`. M75 no-arg `LAST_INSERT_ID()`, M116 `UUID()`, and M115 `JSON_EXTRACT` are unchanged.

```bash
cargo test -p rusql-executor last_insert
cargo test -p rusql-server last_insert
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M116 UUID (2026-09-20)

**What**: `SELECT UUID()` returns a 36-character hyphenated hex string (`8-4-4-4-12`). Two calls on the same connection are not equal. Extra arguments error (i18n). rusql implements RFC 4122 **version 4** (random), not MySQL 8.0's time-based version 1, so values will not match Docker MySQL. This is not `UUID_TO_BIN`, `BIN_TO_UUID`, `UUID_SHORT`, or `SYS_GUID`. M115 `JSON_EXTRACT` and M113 builtins are unchanged.

```bash
cargo test -p rusql-executor uuid
cargo test -p rusql-server uuid
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M115 JSON_EXTRACT (2026-09-20)

**What**: `SELECT JSON_EXTRACT('{"a":1}', '$.a')` returns MySQL 8.0's unquoted `1`. Nested object paths such as `$.a.b` work. A missing path (`$.nope`) is SQL NULL (empty cell), not an error. Invalid JSON is errno 3141 (`ER_INVALID_JSON_TEXT`) with an i18n message. This is not `JSON_SET`, `->` / `->>`, `JSON_OBJECT`, `JSON_TABLE`, or full JSONPath. M113 `SUBSTRING`/`ROUND`/`DATE_ADD` and M46 builtins are unchanged.

```bash
cargo test -p rusql-executor json_extract
cargo test -p rusql-server json_extract
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: Phases S–Z filed (2026-09-20)

**What**: Later full-parity stages are now GitHub issues: Phase S JSON/query pack [#285](https://github.com/tanbamboo/rusql/issues/285)–[#297](https://github.com/tanbamboo/rusql/issues/297) through Phase Z remaining surface [#349](https://github.com/tanbamboo/rusql/issues/349)–[#363](https://github.com/tanbamboo/rusql/issues/363) (milestones 10–17). Later-phase issues are not `agent-ready`. rusql is still **not** a MySQL 8.0 drop-in; M209/M210 remain the definition of done.

```bash
node scripts/create-phase-s-z-issues.mjs
gh issue list --repo tanbamboo/rusql --milestone "Phase S — JSON/query pack (M133–M145)"
```

See [mysql-full-parity-roadmap.md](specs/mysql-full-parity-roadmap.md) and [rusql vs MySQL](reports/rusql-vs-mysql.md).

---

## Latest: M114 CREATE DATABASE CHARACTER SET (2026-09-20)

**What**: `CREATE DATABASE gap_cs CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci` succeeds (`CHARSET` and optional `DEFAULT` too). `SHOW CREATE DATABASE gap_cs` and `information_schema.SCHEMATA` report the stored charset/collation instead of a hardcoded stub. Omitting the clauses keeps rusql defaults (`utf8mb4` / `utf8mb4_unicode_ci`). Supported collations are `utf8mb4_unicode_ci` and `utf8mb4_0900_ai_ci`. Unknown charset is errno 1115; unknown collation is errno 1273. This is not `ALTER DATABASE … CHARACTER SET` and not every MySQL charset. M91 column names are unchanged.

```bash
cargo test -p rusql-sql create_database
cargo test -p rusql-storage create_database
cargo test -p rusql-executor create_database
cargo test -p rusql-server create_database
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: Phase R filed (2026-09-20)

**What**: Post-Q gap-probe work is now Phase R (M114–M132): 19 GitHub issues [#265](https://github.com/tanbamboo/rusql/issues/265)–[#283](https://github.com/tanbamboo/rusql/issues/283) on milestone [Phase R](https://github.com/tanbamboo/rusql/milestone/9). The [full parity roadmap](specs/mysql-full-parity-roadmap.md) also specifies later stages S–Z (JSON pack, schema, locking, programs, replication, security, observability, remaining engine) through M210. rusql is still **not** a MySQL 8.0 drop-in.

```bash
gh issue list --repo tanbamboo/rusql --milestone "Phase R — Post-Q client SQL (M114–M132)"
node scripts/create-phase-r-issues.mjs
```

See [rusql vs MySQL](reports/rusql-vs-mysql.md).

---

## Latest: Phase Q complete (2026-09-20)

**What**: Filed Phase Q table M62–M113 is on `main` (last: M113 [PR #263](https://github.com/tanbamboo/rusql/pull/263)). Official MySQL CLI session introspection (`DATABASE`/`USER`/`VERSION`/`CONNECTION_ID`/`@@`/`SHOW VARIABLES`/`SET NAMES`) returns no `unsupported function`. Gap probe after M113: 29 probes, 19 remaining rusql gaps (post-Q; not production drop-in).

```bash
node scripts/mysql-gap-probe.mjs
```

See [rusql vs MySQL](reports/rusql-vs-mysql.md) and [mysql-full-parity-roadmap.md](specs/mysql-full-parity-roadmap.md).

---

## Latest: M113 SUBSTRING / ROUND / DATE_ADD (2026-09-20)

**What**: `SELECT SUBSTRING('abc', 1, 2)` and `SELECT SUBSTR('abc', 1, 2)` return `ab` (MySQL 1-based). `ROUND(1.4)` is `1` and `ROUND(1.5)` is `2` using half-away-from-zero (not banker's rounding; values parsed as `f64`). `DATE_ADD('2026-01-01', INTERVAL 1 DAY)` returns `2026-01-02`. Units `DAY`/`HOUR`/`MINUTE`/`SECOND` (and `WEEK`) add exact seconds; `MONTH`/`YEAR` reuse the M105 30/365-day approximation. Date-only input plus a calendar unit returns a date string; otherwise a datetime stamp. `JSON_EXTRACT` / `UUID()` / `GET_LOCK` / `LAST_INSERT_ID(expr)` stay unsupported. M46 builtins are unchanged.

```bash
cargo test -p rusql-executor substring
cargo test -p rusql-executor round
cargo test -p rusql-executor date_add
cargo test -p rusql-server substring
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M112 INSERT IGNORE (2026-09-20)

**What**: `INSERT IGNORE INTO t VALUES (…)` skips a PRIMARY KEY conflict (`affected_rows` is rows actually inserted; the existing row is unchanged). A new PK inserts as usual (`affected_rows` 1). Multi-row `INSERT IGNORE` inserts non-conflicting rows and skips duplicates. Plain `INSERT` of a duplicate PK stays errno 1062. M68 `ON DUPLICATE KEY UPDATE` and M111 `REPLACE INTO` are unchanged. This is not UNIQUE-not-PK IGNORE, not sql_mode truncation IGNORE, and not `SHOW WARNINGS` notes.

```bash
cargo test -p rusql-executor insert_ignore
cargo test -p rusql-server insert_ignore
```
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M111 REPLACE INTO (2026-09-20)

**What**: `REPLACE INTO t VALUES (…)` inserts when the PRIMARY KEY is new (`affected_rows` 1) and, on a single-column PK conflict, deletes the old row then inserts the new values (`affected_rows` 2). Composite PRIMARY KEY is rejected (same limit as M68). Plain `INSERT` of a duplicate PK stays errno 1062. `INSERT … ON DUPLICATE KEY UPDATE` is unchanged. `INSERT IGNORE` is still unsupported. This is not multi-table REPLACE and not UNIQUE-not-PK conflict handling.

```bash
cargo test -p rusql-executor replace
cargo test -p rusql-server replace
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M110 TRUNCATE TABLE (2026-09-20)

**What**: `TRUNCATE TABLE t` and `TRUNCATE t` remove all rows from base table `t` (heap delete-all) and reset `AUTO_INCREMENT` to 1 when the table has an auto-increment column. The OK packet reports `affected_rows = 0`. Unknown tables are errno 1146. `information_schema` / SHOW virtual tables and views are rejected. `AFTER DELETE` triggers do not fire. This is not InnoDB tablespace reuse and not `TRUNCATE … PARTITION` / `CASCADE`. `DELETE` / `DROP TABLE` are unchanged.

```bash
cargo test -p rusql-executor truncate
cargo test -p rusql-server truncate
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M109 information_schema.EVENTS (2026-09-20)

**What**: `SELECT EVENT_NAME FROM information_schema.EVENTS` (and `information_schema.events`) returns one row per catalog event instead of errno 1146. Documented columns include `EVENT_SCHEMA`, `EVENT_NAME`, `DEFINER` (empty → stub `root@%`), `EVENT_TYPE`, `EXECUTE_AT`, `INTERVAL_VALUE`, `INTERVAL_FIELD`, `STARTS`, `ENDS`, `STATUS`, `ON_COMPLETION` (unset → `NOT PRESERVE`), `LAST_EXECUTED` (empty when never run), and `EVENT_COMMENT` (empty when unset). This is not a timer thread, not extra `SHOW EVENTS` columns, and not invented timestamps. M108 COMMENT, M107 DEFINER / ON COMPLETION, and `SHOW CREATE EVENT` reconstruction are unchanged.

```bash
cargo test -p rusql-executor information_schema
cargo test -p rusql-executor events
cargo test -p rusql-server information_schema
```

See [user-guide.md](user-guide.md) and `node scripts/check-changelog.mjs`.

---

## Latest: M108 Event COMMENT persistence (2026-09-20)

**What**: `CREATE EVENT … COMMENT 'text' DO …` and `ALTER EVENT … COMMENT 'text'` persist comment text. `SHOW CREATE EVENT` reconstructs `COMMENT '…'` when set and omits the clause when empty/unset (MySQL default). `SHOW EVENTS` stays 15 columns. This is not `information_schema.EVENTS` or COMMENT on procedures/functions/triggers/views. M107 DEFINER / ON COMPLETION, M106 `STARTS`/`ENDS`, and `SHOW CREATE USER` from M99 are unchanged.

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-sql alter_event
cargo test -p rusql-core programs
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server create_event
```

---

## Latest: rusql vs MySQL compatibility report (2026-09-20)

**What**: User-facing test report comparing rusql to MySQL 8.0: production verdict (not a drop-in), what works / is stubbed / is missing, and the measured suites (`mysql-diff` 297/297, gap probe, mysql-test subset).

See [rusql-vs-mysql.md](reports/rusql-vs-mysql.md) and [user-guide.md](user-guide.md). Changelog check: `node scripts/check-changelog.mjs`.

---

## Latest: mysql-gap-probe inventory (2026-09-20)

**What**: `node scripts/mysql-gap-probe.mjs` runs a client-SQL inventory against rusql (optional Docker MySQL 8.0). It always exits 0 (not a CI gate). Phase Q issues M108–M113 were filed from the 2026-09-20 run. Next: implement [M108](https://github.com/tanbamboo/rusql/issues/250).

```bash
node scripts/mysql-gap-probe.mjs
```

See [user-guide.md](user-guide.md) development sensors and `node scripts/check-changelog.mjs`.

---

## Latest: M107 Event DEFINER / ON COMPLETION (2026-09-20)

**What**: `CREATE [DEFINER = user] EVENT … [ON COMPLETION [NOT] PRESERVE]` persists definer and completion. `SHOW EVENTS` `Definer` comes from the catalog; `SHOW CREATE EVENT` reconstructs `DEFINER=\`u\`@\`h\`` and `ON COMPLETION PRESERVE|NOT PRESERVE`. Due `AT` with `PRESERVE` stays `DISABLED` after run; `NOT PRESERVE` still drops. This is not a timer thread. M106 `STARTS`/`ENDS`, M105 watermark, and `SHOW CREATE USER` from M99 are unchanged.

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-sql alter_event
cargo test -p rusql-core programs
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server event_scheduler
```

---

## Latest: M106 Event scheduler STARTS / ENDS (2026-09-19)

**What**: ENABLED `EVERY` events are not due when `now < starts` or `now > ends` (inclusive window). `CREATE EVENT … EVERY n UNIT STARTS 'ts' [ENDS 'ts']` and `ALTER EVENT … STARTS` / `ENDS` persist the timestamps. `SHOW EVENTS` `Starts` / `Ends` come from the catalog; `SHOW CREATE EVENT` reconstructs them. Column count stays 15. This is not a timer thread. M105 interval watermark, M104 one-time `AT` drop-after-run, and `SHOW CREATE USER` from M99 are unchanged.

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-sql alter_event
cargo test -p rusql-core programs
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-server event_scheduler
```

---

## Latest: M105 Event scheduler EVERY (2026-09-19)

**What**: ENABLED `RECURRING` `EVERY n {SECOND|MINUTE|HOUR|DAY|WEEK|MONTH|YEAR}` events run `DO` on the next COM_QUERY, then again after `last_executed + interval`. The catalog row stays (`SHOW EVENTS` lists it; no last-executed column). `MONTH`/`YEAR` use 30/365-day approximations. DISABLED `EVERY` is skipped. M104 one-time `AT` (run then drop) and `SHOW CREATE USER` from M99 are unchanged.

```bash
cargo test -p rusql-core programs
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-server event_scheduler
```

---

## Latest: M104 Event scheduler due AT (2026-09-19)

**What**: ENABLED one-time `AT` events whose `execute_at` is due (UTC `YYYY-MM-DD HH:MM:SS`) run `DO` on the next COM_QUERY, then the catalog row is dropped (`ON COMPLETION NOT PRESERVE`). `@@event_scheduler` / `SHOW VARIABLES LIKE 'event_scheduler'` is a read-only `ON` stub (SET errno 1238; `SET GLOBAL` stays 1229). DISABLED, future `AT`, and `EVERY` events are not run. `DO` errors do not fail the client statement. This is not a timer thread, last-executed timestamps, or DEFINER. `ALTER EVENT` from M103 and `SHOW CREATE USER` from M99 are unchanged.

```bash
cargo test -p rusql-executor event_scheduler
cargo test -p rusql-executor session_var
cargo test -p rusql-server event_scheduler
```

---

## Latest: M103 ALTER EVENT catalog (2026-09-18)

**What**: `ALTER EVENT name` updates catalog `EventMeta` (`ON SCHEDULE AT` / `EVERY n UNIT`, `ENABLE` / `DISABLE`, `RENAME TO`, `DO stmt`). Unknown names return errno 1539. `SHOW EVENTS` and `SHOW CREATE EVENT` reflect the row. The event scheduler does not run `DO`. This is not DEFINER / ON COMPLETION / COMMENT persistence. `SHOW CREATE USER` from M99 and `SHOW FUNCTION STATUS` from M98 are unchanged.

```bash
cargo test -p rusql-sql alter_event
cargo test -p rusql-executor alter_event
cargo test -p rusql-server alter_event
```

---

## Latest: M102 CREATE EVENT catalog (2026-09-18)

**What**: `CREATE EVENT name ON SCHEDULE AT 'timestamp' DO stmt` and `CREATE EVENT name ON SCHEDULE EVERY n {SECOND|MINUTE|HOUR|DAY|WEEK|MONTH|YEAR} DO stmt` persist `EventMeta` in `{data_dir}/programs.json`. `SHOW EVENTS` lists catalog rows (`Db`/`Name`/`Type`/schedule from the catalog; Definer/timezone/charset/Originator are stubs). `SHOW CREATE EVENT` reconstructs `CREATE EVENT \`name\` ON SCHEDULE … DO …`. Duplicate names return errno 1537. Unknown names stay errno 1539. `DROP EVENT` (optional `IF EXISTS`) removes the row. The event scheduler does not run `DO`. This is not `ALTER EVENT`. `SHOW CREATE USER` from M99 and `SHOW FUNCTION STATUS` from M98 are unchanged.

```bash
cargo test -p rusql-sql create_event
cargo test -p rusql-executor create_event
cargo test -p rusql-executor show_events
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server create_event
```

---

## Latest: M101 SHOW EVENTS stubs (2026-09-18)

**What**: `SHOW EVENTS` (optional `FROM`/`IN` db and `LIKE`) returns MySQL-shaped columns (`Db`, `Name`, `Definer`, `Time zone`, `Type`, `Execute at`, `Interval value`, `Interval field`, `Starts`, `Ends`, `Status`, `Originator`, `character_set_client`, `collation_connection`, `Database Collation`) over an empty catalog. rusql has no event scheduler yet, so the result is zero rows (not a parse error). Unmatched `LIKE` returns zero rows. Unknown `FROM` databases return errno 1049. This is not `CREATE EVENT` and not reconstructed `SHOW CREATE EVENT` DDL. `SHOW CREATE EVENT` from M100, `SHOW CREATE USER` from M99, and `SHOW FUNCTION STATUS` from M98 are unchanged.

```bash
cargo test -p rusql-sql show_events
cargo test -p rusql-executor show_events
cargo test -p rusql-server show_events
```

---

## Latest: M100 SHOW CREATE EVENT stubs (2026-09-18)

**What**: `SHOW CREATE EVENT name` (optional `db.name`) is accepted instead of a parse / unsupported-statement error. rusql has no event scheduler catalog yet, so every name returns errno 1539 (`ER_EVENT_DOES_NOT_EXIST`). This is not reconstructed event DDL, not `CREATE EVENT`, and not `SHOW EVENTS`. `SHOW CREATE USER` from M99, `SHOW FUNCTION STATUS` from M98, and `SHOW PROCEDURE STATUS` from M97 are unchanged.

```bash
cargo test -p rusql-sql show_create_event
cargo test -p rusql-executor show_create_event
cargo test -p rusql-server show_create_event
```

---

## Latest: M99 SHOW CREATE USER stubs (2026-09-18)

**What**: `SHOW CREATE USER` (`'u'@'h'`, `user@host`, or `CURRENT_USER`) returns a MySQL-shaped column (`CREATE USER for {user}@{host}`). The cell is reconstructed from the M55 account catalog (`CREATE USER \`u\`@\`h\` IDENTIFIED WITH '{plugin}'`) with no password hash, `BY`, or `AS` clause. Unknown accounts return errno 3162. This is not TLS / resource-limit / DEFAULT ROLE dump. `SHOW FUNCTION STATUS` from M98, `SHOW PROCEDURE STATUS` from M97, and `SHOW CREATE FUNCTION` from M96 are unchanged.

```bash
cargo test -p rusql-sql show_create_user
cargo test -p rusql-executor show_create_user
cargo test -p rusql-server show_create_user
```

---

## Latest: M98 SHOW FUNCTION STATUS stubs (2026-09-18)

**What**: `SHOW FUNCTION STATUS` (optional `LIKE`) returns MySQL-shaped columns (`Db`, `Name`, `Type`, `Definer`, `Modified`, `Created`, `Security_type`, `Comment`, `character_set_client`, `collation_connection`, `Database Collation`). `Db` / `Name` come from catalog `FunctionMeta`; `Type` is `FUNCTION`. Definer / timestamps / charset are documented stubs (`root@%`, empty `Modified`/`Created`/`Comment`, `DEFINER`, `utf8mb4` / `utf8mb4_unicode_ci`). Unmatched `LIKE` returns zero rows. This is not live DEFINER persistence. `SHOW PROCEDURE STATUS` from M97, `SHOW CREATE FUNCTION` from M96, and `SHOW CREATE PROCEDURE` from M95 are unchanged.

```bash
cargo test -p rusql-sql show_function_status
cargo test -p rusql-executor show_function_status
cargo test -p rusql-server show_function_status
```

---

## Latest: M97 SHOW PROCEDURE STATUS stubs (2026-09-18)

**What**: `SHOW PROCEDURE STATUS` (optional `LIKE`) returns MySQL-shaped columns (`Db`, `Name`, `Type`, `Definer`, `Modified`, `Created`, `Security_type`, `Comment`, `character_set_client`, `collation_connection`, `Database Collation`). `Db` / `Name` come from catalog `ProcedureMeta`; `Type` is `PROCEDURE`. Definer / timestamps / charset are documented stubs (`root@%`, empty `Modified`/`Created`/`Comment`, `DEFINER`, `utf8mb4` / `utf8mb4_unicode_ci`). Unmatched `LIKE` returns zero rows. This is not live DEFINER persistence and not `SHOW FUNCTION STATUS`. `SHOW CREATE FUNCTION` from M96, `SHOW CREATE PROCEDURE` from M95, and `SHOW CREATE TRIGGER` from M94 are unchanged.

```bash
cargo test -p rusql-sql show_procedure_status
cargo test -p rusql-executor show_procedure_status
cargo test -p rusql-server show_procedure_status
```

---

## Latest: M96 SHOW CREATE FUNCTION stubs (2026-09-18)

**What**: `SHOW CREATE FUNCTION` returns MySQL-shaped columns (`Function`, `sql_mode`, `Create Function`, `character_set_client`, `collation_connection`, `Database Collation`). The `Create Function` cell is reconstructed from catalog `FunctionMeta` (`CREATE FUNCTION \`f\`() RETURNS … BEGIN RETURN … END` with an empty parameter list). `sql_mode` / charset are documented stubs (empty `sql_mode`, `utf8mb4` / `utf8mb4_unicode_ci`). Unknown functions return errno 1305. This is not DEFINER / sql_mode dump and not invented IN/OUT params. `SHOW CREATE PROCEDURE` from M95, `SHOW CREATE TRIGGER` from M94, and `SHOW CREATE VIEW` from M92 are unchanged.

```bash
cargo test -p rusql-sql show_create_function
cargo test -p rusql-executor show_create_function
cargo test -p rusql-server show_create_function
```

---

## Latest: M95 SHOW CREATE PROCEDURE stubs (2026-09-17)

**What**: `SHOW CREATE PROCEDURE` returns MySQL-shaped columns (`Procedure`, `sql_mode`, `Create Procedure`, `character_set_client`, `collation_connection`, `Database Collation`). The `Create Procedure` cell is reconstructed from catalog `ProcedureMeta` (`CREATE PROCEDURE \`p\`() BEGIN … END` with an empty parameter list). `sql_mode` / charset are documented stubs (empty `sql_mode`, `utf8mb4` / `utf8mb4_unicode_ci`). Unknown procedures return errno 1305. This is not DEFINER / sql_mode dump and not invented IN/OUT params. `SHOW CREATE TRIGGER` from M94, `SHOW TRIGGERS` from M93, and `SHOW CREATE VIEW` from M92 are unchanged.

```bash
cargo test -p rusql-sql show_create_procedure
cargo test -p rusql-executor show_create_procedure
cargo test -p rusql-server show_create_procedure
```

---

## Latest: M94 SHOW CREATE TRIGGER stubs (2026-09-17)

**What**: `SHOW CREATE TRIGGER` returns MySQL-shaped columns (`Trigger`, `sql_mode`, `SQL Original Statement`, `character_set_client`, `collation_connection`, `Database Collation`, `Created`). The `SQL Original Statement` cell is reconstructed from catalog `TriggerMeta` (`CREATE TRIGGER \`t\` {BEFORE|AFTER} {INSERT|UPDATE|DELETE} ON \`table\` FOR EACH ROW …`). `sql_mode` / charset / `Created` are documented stubs (empty `sql_mode`/`Created`, `utf8mb4` / `utf8mb4_unicode_ci`). Unknown triggers return errno 1360. This is not DEFINER / sql_mode dump and not a full mysqldump. `SHOW TRIGGERS` from M93, `SHOW CREATE VIEW` from M92, and `SHOW CREATE TABLE` from M13 are unchanged.

```bash
cargo test -p rusql-sql show_create_trigger
cargo test -p rusql-executor show_create_trigger
cargo test -p rusql-server show_create_trigger
```

---

## Latest: M93 SHOW TRIGGERS stubs (2026-09-17)

**What**: `SHOW TRIGGERS` (optional `FROM`/`IN` db and `LIKE`) returns MySQL-shaped columns (`Trigger`, `Event`, `Table`, `Statement`, `Timing`, `Created`, `sql_mode`, `Definer`, `character_set_client`, `collation_connection`, `Database Collation`). Catalog cells come from M48 `TriggerMeta`; Definer / sql_mode / charset are documented stubs (`root@%`, empty `Created`/`sql_mode`, `utf8mb4` / `utf8mb4_unicode_ci`). Unmatched `LIKE` returns zero rows. Unknown `FROM` databases return errno 1049. This is not `SHOW CREATE TRIGGER` and not live DEFINER persistence. `SHOW CREATE VIEW` from M92, `SHOW CREATE TABLE` from M13, and `SHOW CREATE DATABASE` from M91 are unchanged.

```bash
cargo test -p rusql-sql show_triggers
cargo test -p rusql-executor show_triggers
cargo test -p rusql-server show_triggers
```

---

## Latest: M92 SHOW CREATE VIEW stubs (2026-09-17)

**What**: `SHOW CREATE VIEW` returns MySQL-shaped columns (`View`, `Create View`, `character_set_client`, `collation_connection`). The `Create View` cell is reconstructed from the catalog SELECT (`CREATE VIEW \`v\` AS …`). `character_set_client` / `collation_connection` are documented stubs (`utf8mb4` / `utf8mb4_unicode_ci`). Unknown views return errno 1146. This is not ALGORITHM / DEFINER / SQL SECURITY and not a full mysqld dump. `SHOW CREATE TABLE` from M13, `SHOW CREATE DATABASE` from M91, and `SHOW WARNINGS` from M90 are unchanged.

```bash
cargo test -p rusql-sql show_create_view
cargo test -p rusql-executor show_create_view
cargo test -p rusql-server show_create_view
```

---

## Latest: M91 SHOW CREATE DATABASE stubs (2026-09-17)

**What**: `SHOW CREATE DATABASE` and `SHOW CREATE SCHEMA` return MySQL-shaped columns (`Database`, `Create Database`) with documented stub DDL that includes `utf8mb4` and rusql's default collation `utf8mb4_unicode_ci`. Unknown databases return errno 1049. This is not a live per-schema charset catalog, not `CREATE DATABASE … CHARACTER SET` / `COLLATE`, and not a full mysqld dump. `SHOW CREATE TABLE` from M13 and `SHOW WARNINGS` from M90 are unchanged.

```bash
cargo test -p rusql-sql show_create_database
cargo test -p rusql-executor show_create_database
cargo test -p rusql-server show_create_database
```

---

## Latest: M90 SHOW WARNINGS stubs (2026-09-17)

**What**: `SHOW WARNINGS` and `SHOW ERRORS` return MySQL-shaped columns (`Level`, `Code`, `Message`) as a documented empty diagnostic list. After a successful statement (for example `SELECT 1`) the result is zero rows, not an unsupported-statement error. This is not live truncation / sql_mode / note generation. `SHOW COUNT(*) WARNINGS`, `LIMIT`, and `WHERE` are not implemented. `SHOW CHARACTER SET` from M89 and `SHOW ENGINES` from M88 are unchanged.

```bash
cargo test -p rusql-sql warnings
cargo test -p rusql-executor warnings
cargo test -p rusql-server warnings
```

---

## Latest: M89 SHOW CHARACTER SET stubs (2026-09-17)

**What**: `SHOW CHARACTER SET` and `SHOW CHARSET` return MySQL-shaped columns (`Charset`, `Description`, `Default collation`, `Maxlen`) for a documented stub set: `utf8mb4` (`Maxlen` = `4`, `Default collation` = `utf8mb4_unicode_ci`, matching M87 `SHOW TABLE STATUS` `Collation`). `SHOW CHARACTER SET LIKE 'utf8%'` filters that set; a non-matching pattern returns zero rows. This is not the full MySQL 8.0 charset catalog and not wire encoding conversion. `SHOW COLLATION` from M59, `SHOW ENGINES` from M88, and `SET CHARACTER SET` from M83 are unchanged.

```bash
cargo test -p rusql-sql character_set
cargo test -p rusql-executor character_set
cargo test -p rusql-server character_set
```

---

## Latest: M88 SHOW ENGINES stubs (2026-09-17)

**What**: `SHOW ENGINES` and `SHOW STORAGE ENGINES` return MySQL-shaped columns (`Engine`, `Support`, `Comment`, `Transactions`, `XA`, `Savepoints`) for a documented stub set: `InnoDB` (`Support` = `DEFAULT`, matching M87 `SHOW TABLE STATUS`), `MEMORY`, `MyISAM`, and `PERFORMANCE_SCHEMA` (`YES`). Comments and YES/NO flags are constants, not live plugins. rusql does not switch engines. This is not the full MySQL 8.0 engine catalog. `SHOW TABLE STATUS` from M87 and `SHOW STATUS` from M86 are unchanged. `SHOW ENGINE INNODB STATUS` is not implemented.

```bash
cargo test -p rusql-sql show_engines
cargo test -p rusql-executor show_engines
cargo test -p rusql-server show_engines
```

---

## Latest: M87 SHOW TABLE STATUS stubs (2026-09-17)

**What**: `SHOW TABLE STATUS` (and `SHOW TABLE STATUS LIKE '…'`, optional `FROM`/`IN` db) returns one row per table in the current database with MySQL-shaped columns (`Name`, `Engine`, `Version`, `Row_format`, `Rows`, `Avg_row_length`, `Data_length`, `Max_data_length`, `Index_length`, `Data_free`, `Auto_increment`, `Create_time`, `Update_time`, `Check_time`, `Collation`, `Checksum`, `Create_options`, `Comment`). `Name` matches `SHOW TABLES`. `Engine` is the documented stub `InnoDB`; `Version`/`Row_format` are `10`/`Dynamic`; `Rows` is the current heap row count; `Auto_increment` follows the table counter when the table has one; `Collation` is `utf8mb4_unicode_ci`. Other numeric/time/comment cells are `0` or empty. This is not live InnoDB file-per-table stats. A non-matching `LIKE` returns zero rows. `SHOW STATUS` from M86 is unchanged. `SHOW ENGINE INNODB STATUS` is not implemented.

```bash
cargo test -p rusql-sql table_status
cargo test -p rusql-executor table_status
cargo test -p rusql-server table_status
```

---

## Latest: M86 SHOW STATUS stubs (2026-09-17)

**What**: `SHOW STATUS`, `SHOW SESSION STATUS`, and `SHOW GLOBAL STATUS` return `Variable_name`/`Value` rows for a documented stub set: `Uptime`, `Threads_connected`, `Threads_running`, `Questions`, `Slow_queries`, `Open_tables`, `Connections`, `Aborted_connects`, `Bytes_received`, `Bytes_sent`. Values are stable constants (`0` or `1`) except `Threads_connected`, which is the current connection-registry count. `SHOW STATUS LIKE 'Threads%'` filters that set; a non-matching pattern returns zero rows. Session and global lists are the same for this slice. This is not the full MySQL 8.0 catalog / `performance_schema`. `SELECT … FOR UPDATE` from M85 and `SET TRANSACTION ISOLATION LEVEL` from M84 are unchanged.

```bash
cargo test -p rusql-executor show_status
cargo test -p rusql-server show_status
```

---

## Latest: M85 SELECT … FOR UPDATE (2026-09-16)

**What**: `SELECT … FOR UPDATE`, `FOR SHARE`, and `LOCK IN SHARE MODE` (plus `NOWAIT` / `SKIP LOCKED`) are accepted and return the same rows as the unlocked `SELECT`. rusql does not take row locks, wait, or skip locked rows. Two connections can both `SELECT … FOR UPDATE` the same row. `SET TRANSACTION ISOLATION LEVEL` overlays from M84 are unchanged; DML stays snapshot isolation.

```bash
cargo test -p rusql-executor for_update
cargo test -p rusql-server for_update
```

---

## Latest: M84 SET TRANSACTION ISOLATION LEVEL (2026-09-16)

**What**: `SET TRANSACTION ISOLATION LEVEL …` and `SET SESSION TRANSACTION ISOLATION LEVEL …` overlay `@@transaction_isolation` and `@@tx_isolation` on that connection (hyphenated MySQL names: `READ-COMMITTED`, `REPEATABLE-READ`, `SERIALIZABLE`, `READ-UNCOMMITTED`). `SHOW VARIABLES` / `SHOW SESSION VARIABLES` see the overlay; other connections and `SHOW GLOBAL VARIABLES` keep `REPEATABLE-READ`. rusql treats next-transaction `SET TRANSACTION` the same as `SET SESSION TRANSACTION` (in-memory overlay only; DML stays snapshot isolation). `SET GLOBAL TRANSACTION` is rejected (errno 1229). Overlays are not WAL. `COM_RESET_CONNECTION` / `COM_CHANGE_USER` restore defaults. `SET NAMES`, `SET CHARACTER SET`, and `@foo :=` from M82/M83 are unchanged.

```bash
cargo test -p rusql-executor set_transaction
cargo test -p rusql-server set_transaction
```

---

## Latest: M83 SET CHARACTER SET / SELECT @foo := expr (2026-09-16)

**What**: `SET CHARACTER SET charset` and `SET CHARSET charset` overlay the same `@@character_set_client` / `connection` / `results` stubs as `SET NAMES` (packet encoding is unchanged). `SELECT @foo := expr` assigns on that connection and returns the value; a later `SELECT @foo` sees it. Unset user variables remain an empty cell (NULL). Overlays are in-memory (not WAL). `COM_RESET_CONNECTION` / `COM_CHANGE_USER` still clear user variables and restore charset stubs. `SET NAMES` / `SET @foo = expr` from M82 are unchanged.

```bash
cargo test -p rusql-executor set_charset
cargo test -p rusql-executor user_var
cargo test -p rusql-server set_charset
cargo test -p rusql-server user_var
```

---

## Latest: M82 SET NAMES / user variables @foo (2026-09-16)

**What**: `SET NAMES charset [COLLATE collation]` overlays `@@character_set_client` / `connection` / `results` (and `@@collation_connection` when `COLLATE` is given, or `utf8mb4_0900_ai_ci` for `utf8mb4`). `SET NAMES DEFAULT` restores documented charset stubs. `SET @foo = expr` then `SELECT @foo` returns the value on that connection; unset `@bar` is an empty cell (NULL). Overlays are in-memory (not WAL). `COM_RESET_CONNECTION` / `COM_CHANGE_USER` clear user variables and restore `SET NAMES` defaults. Packet encoding is unchanged. `SELECT @foo := expr` assignment expressions remain unimplemented.

```bash
cargo test -p rusql-executor set_names
cargo test -p rusql-executor user_var
cargo test -p rusql-server set_names
cargo test -p rusql-server user_var
```

---


**What**: `SET @@var`, `SET @@session.var`, `SET SESSION var`, and `SET var` overlay the documented M77+M79 stub catalog on that connection. `SELECT @@var` / `SELECT @@session.var` and `SHOW VARIABLES` / `SHOW SESSION VARIABLES` see the new value; `SHOW GLOBAL VARIABLES` and other connections keep documented defaults. Overlays are in-memory (not WAL). `COM_RESET_CONNECTION` and `COM_CHANGE_USER` restore defaults. Unknown names still return errno 1193. `SET GLOBAL` is rejected (errno 1229). Read-only stubs `version`, `version_comment`, `license`, and `system_time_zone` reject SET (errno 1238). Autocommit DML behavior is unchanged (still autocommit-on). `SET NAMES` and user variables `@foo` remain unimplemented.

```bash
cargo test -p rusql-executor set_session_var
cargo test -p rusql-server set_session_var
```

---

## Latest: M80 SHOW VARIABLES stub catalog (2026-09-16)

**What**: `SHOW VARIABLES`, `SHOW SESSION VARIABLES`, and `SHOW GLOBAL VARIABLES` return `Variable_name`/`Value` rows for the documented M77+M79 stub set (including `tx_isolation`). `SHOW VARIABLES LIKE 'auto_increment%'` filters that set; a non-matching pattern returns zero rows. Session and global lists are the same for this slice (stubs are not persisted). This is not the full MySQL 8.0 catalog. `SET @@` and user variables `@foo` remain unimplemented.

```bash
cargo test -p rusql-executor show_variables
cargo test -p rusql-server show_variables
```

---

## Latest: M79 more @@ connector probes (2026-09-15)

**What**: JDBC/ORM handshake stubs in addition to M77: `@@auto_increment_increment` (`1`), `@@time_zone` (`SYSTEM`), `@@system_time_zone` (`UTC`, not the host TZ), `@@transaction_isolation` / `@@tx_isolation` (`REPEATABLE-READ`), `@@max_allowed_packet` (`67108864`), `@@license` (`GPL`). `@@session.var` equals `@@var`. Unknown names still return errno 1193. `SET @@`, `@foo`, and `SHOW VARIABLES` are not implemented.

```bash
cargo test -p rusql-executor session_var
cargo test -p rusql-server session_var
```

---

## Latest: M78 FOUND_ROWS() / SQL_CALC_FOUND_ROWS (2026-09-15)

**What**: `SELECT FOUND_ROWS()` after a plain `SELECT` returns that result’s row count (after `LIMIT`). `SELECT SQL_CALC_FOUND_ROWS … LIMIT n` then `FOUND_ROWS()` returns the un-LIMITed match count (deprecated in MySQL 8.0.17; `COUNT(*)` remains the supported alternative). After DML it matches `ROW_COUNT()`. Session-scoped; `COM_RESET_CONNECTION` / `COM_CHANGE_USER` reset to `0`.

```bash
cargo test -p rusql-executor found_rows
cargo test -p rusql-server found_rows
```

---

## Latest: M77 @@ session variables (2026-09-15)

**What**: `SELECT @@version` matches `VERSION()` (`8.0.33-rusql`). Documented stubs: `@@version_comment`, `@@autocommit` (`1`), `@@character_set_client`/`connection`/`results`/`server` (`utf8mb4`), `@@collation_connection` (`utf8mb4_0900_ai_ci`), `@@sql_mode` (MySQL 8.0-like string, not enforced). `@@session.var` equals `@@var` for this set. Unknown names return errno 1193. `SET @@`, `@foo`, and `SHOW VARIABLES` are not implemented.

```bash
cargo test -p rusql-executor session_var
cargo test -p rusql-server session_var
```

---

## Latest: M76 CONNECTION_ID() / ROW_COUNT() (2026-09-14)

**What**: `SELECT CONNECTION_ID()` returns the connection’s handshake thread id (stable for the session; same value as `SHOW PROCESSLIST` `Id` / `COM_PROCESS_INFO`). `SELECT ROW_COUNT()` returns the affected-row count of the last `INSERT`/`UPDATE`/`DELETE` on that connection, and `-1` after a statement that returns a result set (MySQL). Connections do not share `ROW_COUNT()`; `COM_RESET_CONNECTION` / `COM_CHANGE_USER` reset it to `-1`. `FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS` are not implemented.

```bash
cargo test -p rusql-executor connection_id
cargo test -p rusql-executor row_count
cargo test -p rusql-server connection_id
cargo test -p rusql-server row_count
```

---

## Latest: M75 LAST_INSERT_ID() (2026-09-14)

**What**: `SELECT LAST_INSERT_ID()` returns the first generated `AUTO_INCREMENT` value from the last successful `INSERT` on that connection (`0` if none). The INSERT OK packet `last_insert_id` field matches. Explicit non-generated inserts (and `LAST_INSERT_ID(expr)` setter) are not tracked. Connections do not share the value; `COM_RESET_CONNECTION` / `COM_CHANGE_USER` clear it.

```bash
cargo test -p rusql-executor last_insert
cargo test -p rusql-server last_insert
```

---

## Latest: M74 UPDATE/DELETE row events (2026-09-14)

**What**: Committed `UPDATE`/`DELETE` write `TABLE_MAP_EVENT` then `UPDATE_ROWS_EVENT_V1` (type 24) / `DELETE_ROWS_EVENT_V1` (type 25). Cells are UTF-8 with a u32 length prefix (same as M72 INSERT). `extract`/`apply_binlog_file` reconstruct `UPDATE`/`DELETE` SQL. INSERT row events and live dump follow are unchanged. Replica tables must already exist.

```bash
cargo test -p rusql-storage binlog
cargo test -p rusql-server binlog_dump
```

---

## Latest: M73 live COM_BINLOG_DUMP follow (2026-09-14)

**What**: `COM_BINLOG_DUMP` with flags `0` streams existing events from the requested position and stays open (no OK). Later `COMMIT`s are written as additional `0x00` + event packets (INSERT `TABLE_MAP` + `WRITE_ROWS`, or UPDATE/DELETE `QUERY_EVENT`). `BINLOG_DUMP_NON_BLOCK` (flags `0x01`) keeps the M71 one-shot dump then OK. Client disconnect or `COM_QUIT` ends follow.

```bash
cargo test -p rusql-storage binlog
cargo test -p rusql-server binlog_dump
```

---

## Latest: M72 TABLE_MAP + WRITE_ROWS for INSERT (2026-09-14)

**What**: Committed `INSERT`s write `TABLE_MAP_EVENT` (type 19) then `WRITE_ROWS_EVENT_V1` (type 23) after `FORMAT_DESCRIPTION`. Cells are UTF-8 with a u32 length prefix; empty cells are SQL `NULL`. `extract`/`apply_binlog_file` reconstruct `INSERT INTO … VALUES (…)`. UPDATE/DELETE stay QUERY_EVENT. `COM_BINLOG_DUMP` still sends one packet per event (M71). Replica tables must already exist (DDL is not in the binlog).

```bash
cargo test -p rusql-storage binlog
cargo test -p rusql-server binlog_dump
```

---

## Latest: M71 per-event COM_BINLOG_DUMP (2026-09-14)

**What**: `COM_BINLOG_DUMP` sends each binlog event as its own packet (`0x00` + event bytes) starting at the requested file position. Position `4` yields `FORMAT_DESCRIPTION` first; committed `INSERT`s appear as `QUERY_EVENT`s. One-shot dump of the current file (no live follow).

```bash
cargo test -p rusql-storage binlog
cargo test -p rusql-server binlog_dump
```

---

## Latest: M70 window ranking functions (2026-09-14)

**What**: `ROW_NUMBER()`, `RANK()`, and `DENSE_RANK()` as top-level `SELECT` items with `OVER (PARTITION BY … ORDER BY …)`. `WHERE` runs before the window; outer `ORDER BY` / `LIMIT` after. Window frames (`ROWS`/`RANGE`) and other window functions are rejected.

```bash
cargo test -p rusql-executor window
cargo test -p rusql-server window
```

---

## Latest: M69 non-recursive WITH (CTE) (2026-09-14)

**What**: `WITH cte AS (SELECT …) SELECT … FROM cte` names a subquery. Multiple CTEs can chain (later CTEs may read earlier ones). `WITH RECURSIVE` is rejected.

```bash
cargo test -p rusql-executor with_cte
cargo test -p rusql-server with_cte
```

---

## Latest: M68 INSERT … SELECT / ON DUPLICATE KEY UPDATE (2026-09-14)

**What**: `INSERT INTO dst SELECT … FROM src` copies query results. Duplicate single-column `PRIMARY KEY` returns errno 1062 unless `ON DUPLICATE KEY UPDATE` upserts (`VALUES(col)` and existing-row expressions such as `cnt = cnt + 1`).

```bash
cargo test -p rusql-executor insert_select
cargo test -p rusql-executor on_duplicate
cargo test -p rusql-server insert_select
```

---

## Latest: M67 SELECT DISTINCT (2026-09-07)

**What**: `SELECT DISTINCT` removes duplicate projected rows (MySQL semantics). Applies after projection and before `ORDER BY` / `LIMIT`. `DISTINCT ON` is rejected.

```bash
cargo test -p rusql-executor select_distinct
cargo test -p rusql-server select_distinct
```

---

## Latest: M66 CASE / IF expressions (2026-09-07)

**What**: Searched and simple `CASE … END`, plus `IF(cond, then, else)`, in `SELECT` projections (with or without `FROM`).

```bash
cargo test -p rusql-executor case_searched
cargo test -p rusql-server case_and_if
```

---

## Latest: M65 session info functions (2026-09-07)

**What**: `DATABASE()` / `SCHEMA()`, `USER()` / `CURRENT_USER()` / `SESSION_USER()`, and `VERSION()` return session metadata (MySQL 8.0-compatible `8.0.33-rusql` version string).

```bash
cargo test -p rusql-executor session_info
cargo test -p rusql-server session_info_functions
```

---

## Latest: M62 utf8mb4_0900_ai_ci (2026-09-02)

**What**: `utf8mb4_0900_ai_ci` collation for `CREATE TABLE … COLLATE`, `ORDER BY`, and `WHERE =`; listed in `SHOW COLLATION` alongside `utf8mb4_unicode_ci`.

```bash
cargo test -p rusql-core collation
cargo test -p rusql-executor collation_0900
```

---

## Latest: M63 CREATE FUNCTION (2026-09-01)

**What**: `CREATE FUNCTION f() RETURNS INT BEGIN RETURN 42; END`, `DROP FUNCTION`, scalar calls in `SELECT f()` and `SELECT f() + 1`; `information_schema.ROUTINES` shows `ROUTINE_TYPE = FUNCTION`.

```bash
cargo test -p rusql-sql parse_create_function
cargo test -p rusql-executor create_function
```

---

## Latest: M64 AFTER UPDATE/DELETE triggers (2026-09-01)

**What**: `CREATE TRIGGER … AFTER UPDATE` and `AFTER DELETE` fire side-effect DML with `OLD.col` / `NEW.col` substitution (audit-table pattern).

```bash
cargo test -p rusql-executor trigger
```

---

## Latest: P3 stored programs + replication MVP (2026-09-01)

**What**: `CREATE PROCEDURE` / `CALL` / `DROP PROCEDURE`, `CREATE TRIGGER` (BEFORE INSERT with `SET NEW.col`), binlog QUERY events on `COMMIT` with GTID comment stub, `COM_BINLOG_DUMP` / `COM_REGISTER_SLAVE`, `apply_binlog_file` replica applier, `SHOW MASTER/SLAVE STATUS` GTID stubs.

```bash
cargo test -p rusql-sql stored_programs
cargo test -p rusql-executor programs
cargo test -p rusql-storage binlog
cargo test -p rusql-storage replica
```

---

## Latest: M59 collation + M61 Sysbench (2026-09-01)

**What**: `utf8mb4_unicode_ci` compare/sort for `ORDER BY` and `WHERE =`; Sysbench `oltp_point_select` harness vs Docker MySQL.

```bash
cargo test -p rusql-core collation
cargo test -p rusql-executor collation_order_by
node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308
```

---

## Latest: PERF-B2/B3 query and DML optimizations (2026-09-01)

**What**: Index-ordered scan for `ORDER BY` + `LIMIT` (no `WHERE`); PK-targeted `UPDATE` with incremental index maintenance.

```bash
cargo test -p rusql-storage scan_index_ordered_with_limit pk_update_without_index_rebuild
cargo test -p rusql-executor select_order_by_indexed_limit update_pk_by_index
```

---

## Latest: PERF-B4–B6 performance harness (2026-09-01)

**What**: Multi-thread benchmark (`--threads`, `--thread-matrix`), `--wal-sync` policy, optional Sysbench CI gate.

```bash
node scripts/bench-rusql-vs-mysql.mjs --thread-matrix --compare --rusql-port 3307 --mysql-port 3308
cargo run -p rusql-server -- --wal-sync batch --port 3307 --data-dir ./.test-data-bench
node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308
```

---

## Latest: M51–M53 wire protocol commands (2026-09-01)

**What**: `COM_CHANGE_USER`, `COM_RESET_CONNECTION`, `COM_FIELD_LIST`, prepared-statement long data/reset, and `SHOW PROCESSLIST` / `COM_PROCESS_INFO`.

```bash
cargo test -p rusql-protocol
cargo test -p rusql-server show_processlist
cargo test -p rusql-server com_field_list
cargo test -p rusql-server com_change_user
```

---

## Latest: PERF-B4/B5/B6 concurrency, WAL sync, Sysbench (2026-09-01)

**What**: Multi-threaded benchmark harness, configurable WAL `fsync` policy, and optional Sysbench `oltp_point_select` gate.

```bash
# Multi-thread benchmark
node scripts/bench-rusql-vs-mysql.mjs --thread-matrix --compare --rusql-port 3307 --mysql-port 3308

# WAL sync policy
cargo run -p rusql-server -- --wal-sync batch --port 3307 --data-dir ./.test-data-bench
cargo test -p rusql-storage wal_sync_none

# Sysbench gate (soft-fail if tools missing)
node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308
```

---

## Latest: PERF-B1 persistent-connection benchmark (2026-09-01)

**What**: `scripts/bench-rusql-vs-mysql.mjs` runs the same 7 workloads as the 2026-08-11 CLI baseline using one persistent wire client (no per-query process spawn).

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./.test-data-bench
node scripts/bench-rusql-vs-mysql.mjs --host 127.0.0.1 --port 3307 --label rusql --output target/bench-rusql.json
```

---

## M55-auth multi-user accounts (2026-09-01)

**What**: `CREATE USER` / `DROP USER` with passwords persisted in `mysql.user.json`; login as non-root users via `caching_sha2_password` or `mysql_native_password`.

```bash
cargo test -p rusql-core parse_create_user_ddl
cargo test -p rusql-server auth
```

**Try it** (dev server, then as root):

```sql
CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'secret';
```

Connect with `--default-auth=mysql_native_password` and `-u app -p`.

---

## M50 composite indexes (2026-09-01)

**What**: Multi-column `CREATE INDEX idx ON t (a, b)` with prefix equality lookups, composite EXPLAIN plans, and MySQL-style `Seq_in_index` in SHOW INDEX / `information_schema.STATISTICS`.

```bash
cargo test -p rusql-storage composite_index_lookup
cargo test -p rusql-planner composite_eq
cargo test -p rusql-server run_basic_compat
```

---

## M54 GRANT/REVOKE (2026-08-31)

**What**: MySQL-style privilege grants persisted to `mysql.user.json` in the data directory; `GRANT`/`REVOKE`/`SHOW GRANTS`; unauthorized DML returns errno **1142**. User `root` bypasses all checks.

```bash
cargo test -p rusql-core privileges
cargo test -p rusql-server run_basic_compat
```

Example:

```sql
GRANT SELECT, INSERT ON rusql.* TO app;
SHOW GRANTS FOR app;
REVOKE INSERT ON rusql.* FROM app;
```

---

## M39 FOREIGN KEY + M44 UNION (2026-08-31)

**What**: `UNION`/`UNION ALL` result combination; `FOREIGN KEY` on `CREATE TABLE` with RESTRICT enforcement (MySQL errno 1451/1452) and `information_schema.KEY_COLUMN_USAGE`.

```bash
cargo test -p rusql-executor foreign_key
cargo test -p rusql-server compat_suite
node scripts/mysql-diff.mjs   # requires Docker
```

---

## Latest: P1 SQL gaps M40–M60 (2026-08-31)

**What**: Extended column types, outer joins, GROUP BY/HAVING/aggregates, subqueries, SQL expressions, and 100-case mysql-test wire harness with CI pass floor.

```bash
cargo test -p rusql-executor aggregate
cargo test -p rusql-server mysql_test_subset
node scripts/mysql-diff.mjs   # requires Docker
```

---

## Latest: Issue #73 — metadata EOF + SESSION_TRACK (2026-07-06)

**What**: Completes MySQL 8.0 CLI compat after PR #78. Text/binary resultsets now send metadata EOF/OK between column definitions and rows (#79). OK packets include an empty session-state trailer when `CLIENT_SESSION_TRACK` is negotiated (#80). Command-phase OK responses honor client capabilities.

```bash
cargo test -p rusql-protocol response::tests
cargo test -p rusql-server mysql_cli
node scripts/mysql-diff.mjs   # requires Docker; CI uses apt mysql client
```

---

## Issue #73 — MySQL 8.0 CLI COM_QUERY compat (2026-07-06)

**What**: Official `mysql:8.0` clients negotiate `CLIENT_QUERY_ATTRIBUTES` and `CLIENT_DEPRECATE_EOF`. rusql now strips the query-attributes preamble on `COM_QUERY` and ends text/binary resultsets with an OK packet instead of a legacy EOF when required.

```bash
cargo test -p rusql-protocol command::tests
cargo test -p rusql-server mysql_cli_query_attributes
node scripts/mysql-diff.mjs   # requires Docker
```

---

## M31 — Durable COMMIT WAL (2026-06-30)

**What**: `COMMIT` appends pending transaction records to `rusql.wal` with `sync_data`; `ROLLBACK` discards overlay without WAL writes. Verified across storage replay and wire-protocol tests.

```bash
cargo test -p rusql-storage commit_transaction_survives
cargo test -p rusql-server transaction
```

---

## M30 — mysql-test subset (2026-06-30)

**What**: 12 Oracle mysql-test inspired wire cases in `tests/mysql-test/manifest.json`, run via internal test client. Skips documented in `tests/mysql-test/SKIPS.md`.

```bash
node scripts/mysql-test-subset.mjs
cargo test -p rusql-server mysql_test_subset
```

---

## M29 — mysql-diff runner (2026-06-30)

**What**: `node scripts/mysql-diff.mjs` compares portable SQL in `compat/mysql-diff.json` against Docker MySQL 8.0 and rusql-server (skips without Docker).

```bash
node scripts/mysql-diff.mjs
```

---

## M28 — SHOW INDEX (2026-06-30)

**What**: `SHOW INDEX FROM tbl` (also `SHOW INDEXES`, `SHOW KEYS`) lists PRIMARY and secondary indexes with MySQL column names.

```bash
cargo test -p rusql-sql show_index
cargo test -p rusql-executor show_index
cargo test -p rusql-server compat
```

---

## M27 — information_schema SCHEMATA & STATISTICS (2026-06-30)

**What**: `SELECT * FROM information_schema.SCHEMATA` and `STATISTICS` (PRIMARY + secondary indexes).

```bash
cargo test -p rusql-executor info_schema_schemata
cargo test -p rusql-server compat
```

---

## M26 — caching_sha2 RSA full auth (2026-06-30)

**What**: Non-TLS clients can complete `caching_sha2_password` via RSA public-key exchange when `--auth-password` is set.

```bash
cargo test -p rusql-server accepts_caching_sha2_rsa
cargo test -p rusql-protocol rsa_password_roundtrip
```

---

## M25 — Binary resultset (COM_STMT_EXECUTE) (2026-06-30)

**What**: Prepared-statement SELECT returns binary protocol rows with correct MySQL column types (`INT` as 4-byte LE, `VARCHAR` as lenenc string).

```bash
cargo test -p rusql-protocol binary
cargo test -p rusql-server stmt_prepare_execute_binary
```

---

## M24 — ALTER TABLE ADD COLUMN (2026-06-30)

**What**: `ALTER TABLE t ADD COLUMN c TYPE` (and MySQL shorthand `ADD c TYPE`); existing rows get NULL (empty string) in the new column; WAL replay.

```bash
cargo test -p rusql-executor alter_table_add_column
cargo test -p rusql-server compat
```

---

## M23 — PRIMARY KEY metadata (2026-06-30)

**What**: `PRIMARY KEY` and `NOT NULL` stored in catalog; shown in DESCRIBE / SHOW CREATE TABLE.

```bash
cargo test -p rusql-executor describe_primary_key
cargo test -p rusql-server compat
```

---

## M22 — INNER JOIN (2026-06-30)

**What**: `SELECT ... FROM a INNER JOIN b ON a.col = b.col` (two tables).

```bash
cargo test -p rusql-executor inner_join_two_tables
cargo test -p rusql-server compat
```

---

## M21 — IS NULL / IS NOT NULL (2026-06-30)

**What**: `WHERE col IS NULL` and `IS NOT NULL`; `INSERT … NULL` supported.

```bash
cargo test -p rusql-executor where_is_null
cargo test -p rusql-server compat
```

---

## M20 — WHERE comparisons and AND (2026-06-30)

**What**: `WHERE id > 1`, `id <> 2`, `id = 1 AND name = 'x'` on table SELECT.

```bash
cargo test -p rusql-executor where_comparisons_and
cargo test -p rusql-server compat
```

---

## M19 — SELECT LIMIT OFFSET (2026-06-30)

**What**: `LIMIT n OFFSET m` after ORDER BY / projection on table SELECT.

```sql
SELECT * FROM users ORDER BY id LIMIT 1 OFFSET 1;
```

```bash
cargo test -p rusql-executor select_limit
cargo test -p rusql-server compat
```

---

## M18 — SELECT column aliases (2026-06-30)

**What**: Result set column headers use `AS` aliases (e.g. `SELECT id AS user_id`).

```sql
SELECT id AS user_id FROM users;
```

```bash
cargo test -p rusql-executor select_column_aliases
cargo test -p rusql-server compat
```

---

## M17 — SELECT ORDER BY (2026-06-30)

**What**: `ORDER BY col [ASC|DESC]` on table `SELECT` (after projection/filter, before `LIMIT`).

```sql
SELECT * FROM users ORDER BY id;
SELECT name FROM users ORDER BY name DESC;
```

```bash
cargo test -p rusql-executor select_order_by
cargo test -p rusql-server compat
```

---

## M16 — SELECT LIMIT (2026-06-30)

**What**: `SELECT * FROM tbl LIMIT n` caps result rows (with projection/WHERE).

```bash
cargo test -p rusql-executor select_limit
```

---

## M15 — USE database (2026-06-30)

**What**: `USE rusql` sets session default database; unknown DB names error.

```sql
USE rusql;
```

Note: `USE DATABASE name` is not parsed by our MySQL dialect yet; clients using `USE name` work.

```bash
cargo test -p rusql-executor use_database
cargo test -p rusql-server use_database
```

---

## M14 — SELECT column projection (2026-06-30)

**What**: `SELECT id, name FROM users` returns only listed columns; `SELECT *` unchanged.

**Try it**:

```sql
SELECT name FROM users;
SELECT id, name FROM users WHERE id = 1;
```

**Automated**:

```bash
cargo test -p rusql-executor select_column_projection
cargo test -p rusql-server run_basic_compat_fixtures
```

---

## Book — Harness Engineering narrative (#28)

**What**: mdBook in English and zh-CN — one chapter per milestone (M0–M13), Harness Engineering part, metrics appendix.

**Read**: [docs/book/README.md](../../docs/book/README.md)

**Build**:

```bash
cargo install mdbook   # once
node scripts/build-book.mjs
node scripts/check-book.mjs
```

---

## M13 — SHOW CREATE TABLE (2026-06-30)

**What**: `SHOW CREATE TABLE tbl` returns `Table` and `Create Table` columns with reconstructable DDL.

**Try it**:

```sql
CREATE TABLE users (id INT, name VARCHAR(32));
SHOW CREATE TABLE users;
```

**Automated**:

```bash
cargo test -p rusql-executor show_create
cargo test -p rusql-server run_basic_compat_fixtures
```

---

## M12 — DESCRIBE and information_schema (2026-06-30)

**What**: `DESCRIBE tbl`, `SHOW COLUMNS FROM tbl`, and virtual `information_schema.tables` / `information_schema.columns` for tooling.

**Try it**:

```sql
DESCRIBE users;
SHOW COLUMNS FROM users;
SELECT * FROM information_schema.tables;
SELECT * FROM information_schema.columns WHERE table_name = 'users';
```

**Automated**:

```bash
cargo test -p rusql-executor describe
cargo test -p rusql-executor information_schema
cargo test -p rusql-server describe
cargo test -p rusql-server run_basic_compat_fixtures
```

Spec: [m12-describe-info-schema.md](specs/m12-describe-info-schema.md)

---

## M11 — Prepared statements (2026-06-30)

**What**: `COM_STMT_PREPARE`, `COM_STMT_EXECUTE`, `COM_STMT_CLOSE`. Supports `?` placeholders (text/VARCHAR params).

**Try it** (via wire tests):

```bash
cargo test -p rusql-server stmt_prepare
```

**Automated**:

```bash
cargo test -p rusql-protocol stmt::
cargo test -p rusql-server stmt_
```

Spec: [m11-stmt-prepare.md](specs/m11-stmt-prepare.md)

---

## M10 — SHOW TABLES / SHOW DATABASES (2026-06-30)

**What**: List tables and the default `rusql` database (MySQL-style result columns).

**Try it**:

```sql
CREATE TABLE users (id INT);
SHOW TABLES;
SHOW DATABASES;
```

**Automated**:

```bash
cargo test -p rusql-executor show_tables
cargo test -p rusql-server compat
```

---

## M9 — Transactions (2026-06-30)

**What**: Explicit `BEGIN`, `COMMIT`, and `ROLLBACK`. Uncommitted writes are visible only to the same connection until commit.

**Try it**:

```bash
cargo run -p rusql-server -- --port 3307
```

```sql
CREATE TABLE t (id INT);
BEGIN;
INSERT INTO t VALUES (1);
SELECT * FROM t;
COMMIT;
```

**Automated**:

```bash
cargo test -p rusql-server transaction_commit_and_rollback
cargo test -p rusql-server compat
```

Spec: [m9-transactions.md](specs/m9-transactions.md)

---

## M8 — UPDATE

`UPDATE table SET col = value [WHERE col = value]` with WAL persistence.

```bash
cargo test -p rusql-server compat
```

---

## M7 — caching_sha2_password

Default auth plugin for MySQL 8 clients. Optional `--auth-password` enables verification.

```bash
cargo test -p rusql-protocol
```

Spec: [adr-m7-caching-sha2.md](specs/adr-m7-caching-sha2.md)

---

## M6 — Auth + DROP / DELETE

Password verification and destructive DML.

Spec: [adr-m6-auth-and-dml.md](specs/adr-m6-auth-and-dml.md)

---

## M5 — Compatibility fixtures

JSON-driven wire tests in `crates/rusql-server/compat/basic.json`.

```bash
cargo test -p rusql-server run_basic_compat_fixtures
```

---

## M4 — Indexes

`CREATE INDEX`, point lookup via `WHERE column = literal`.

---

## M3 — Persistence

`--data-dir` and WAL replay across restarts.

```bash
cargo test -p rusql-server persistence_across_connections
```

---

## M2 — COM_QUERY

SQL over MySQL wire protocol.

```bash
cargo test -p rusql-server com_query
```

---

## M1 — Handshake

MySQL v10 handshake.

```bash
cargo test -p rusql-protocol handshake
```

---

## M0 — Harness

Project bootstrap and CI sensors.

```bash
node scripts/harness-validate.mjs
node scripts/metrics.mjs
```

---

## Updating these notes

Every merged PR that changes user-visible behavior must:

1. Add a bullet under `CHANGELOG.md` → `[Unreleased]`
2. Add or update a **Latest** section here (move previous Latest down)
3. Update [user-guide.md](user-guide.md) test steps if needed

Sensor: `node scripts/check-changelog.mjs`

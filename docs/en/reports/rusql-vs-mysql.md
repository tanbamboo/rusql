# rusql vs MySQL 8.0 — Compatibility Test Report

**As of:** 2026-09-20 (`main` after M113 / Phase Q complete)  
**Audience:** anyone asking “can I run my app on rusql instead of MySQL?”  
**简体中文:** [rusql-vs-mysql.md](../../zh-CN/reports/rusql-vs-mysql.md)

This report is the current user-facing comparison. Historical snapshots: [June 2026](database-compat-report-2026-06-30.md), [July 2026](functional-test-report-2026-07-03.md), [August 2026 performance](performance-benchmark-2026-08-11.md).

---

## 1. Verdict: can you use rusql in production?

**No — not as a drop-in replacement for MySQL 8.0.**

rusql speaks the MySQL wire protocol and matches Docker MySQL 8.0 on every portable statement in the current differential suite. That is real progress. It is still a **growing subset**, not a complete engine: many everyday statements error, several `SHOW` catalogs are stubs, locking/`FOR UPDATE` is a no-op, and replication is an MVP dump path, not failover.

| Question | Answer |
|----------|--------|
| Can the official `mysql` CLI connect and run CRUD? | **Yes**, on the supported subset |
| Do JDBC / common connectors handshake? | **Usually yes** (session `@@` stubs exist for probes) |
| Can a typical production schema + ORM migrate unchanged? | **No** — gaps such as JSON extract, text `PREPARE` |
| Can you fail over with GTID / treat rusql as an InnoDB replica? | **No** |
| Should you store production data you cannot lose, under MySQL semantics? | **No** |

### Production-readiness scorecard

| Area | vs MySQL 8.0 | Use in production? |
|------|----------------|--------------------|
| Wire handshake + `COM_QUERY` | Official client works | Yes for experiments |
| Core DML (`INSERT`/`SELECT`/`UPDATE`/`DELETE`) | Matches on tested steps | Yes for simple apps |
| Transactions + WAL restart | `BEGIN`/`COMMIT`/`ROLLBACK`; snapshot isolation | Durable for the subset; **not** InnoDB locking |
| Schema (`CREATE`/`ALTER`/`INDEX`/`FK`) | Common patterns work; `TRUNCATE TABLE` (M110); `REPLACE INTO` (M111); `INSERT IGNORE` (M112) | Dev / subset only |
| Query SQL (JOIN, `GROUP BY`, subquery, `UNION`, CTE) | Core forms match `mysql-diff` | Yes if you stay on those forms |
| Client `SHOW` / `@@` / `information_schema` | Many catalogs are **stubs** | Connectors work; ops dashboards will lie |
| Privileges | `GRANT`/`REVOKE` + `CREATE USER` MVP | Not a hardened security model |
| Replication | Binlog row events + dump follow MVP | **Not HA** |
| SQL functions | Growing builtin set | Missing `JSON_EXTRACT`/`UUID()`/`GET_LOCK`; `SUBSTRING`/`ROUND`/`DATE_ADD` work (M113) |
| Official `mysql-test` (thousands of `.test` files) | **100** portable cases only | Not a completeness claim |

**Reasonable uses today:** local prototypes, teaching, connector smoke tests, contributing to rusql.  
**Not reasonable today:** replacing MySQL in production, restoring arbitrary dumps, running WordPress/Rails/Django without rewriting SQL.

Roadmap estimate of *client-visible* surface (qualitative, not a version number): about **45–70%**. See [mysql-full-parity-roadmap.md](../specs/mysql-full-parity-roadmap.md).

---

## 2. How rusql is tested against MySQL

There is no claim that rusql passes Oracle’s full `mysql-test` suite. These are the actual corpora.

| Suite | What it compares | Size (2026-09-20) | Gate |
|-------|------------------|-------------------|------|
| **`mysql-diff`** | Same SQL on rusql **and** Docker MySQL 8.0 via official `mysql` CLI | **327 steps**, 60 suites + 2 protocol-smoke queries (318 unique texts) | **CI** — last run **327/327** |
| **`mysql-gap-probe`** | Curated “still missing?” statements vs rusql (optional MySQL) | **29 probes** + 15 setup SQL | Inventory only (always exit 0) |
| **`mysql-test-subset`** | Portable slice of Oracle mysql-test, rusql wire client | **100 cases**, 158 SQL steps | **CI** — 100/100 |
| **`basic.json` fixtures** | rusql wire CREATE/INSERT/SELECT/INDEX/WHERE | **18 suites**, 101 steps | `cargo test -p rusql-server compat` |
| **`cargo test`** | Unit + integration (rusql only) | Workspace tests | **CI** |
| **`sysbench-rusql.mjs`** | QPS vs MySQL (`oltp_point_select`) | Few statement shapes, many iterations | Manual / `workflow_dispatch` |
| **`bench-rusql-vs-mysql.mjs`** | Latency/QPS on 7 micro-workloads | Not SQL coverage | Manual |

**Unique SQL texts** across the four JSON corpora: see `mysql-diff.json` (M112 + M113 suites added). That is statement inventory, not “N MySQL features.”

Oracle **mysql-test** remains **thousands** of `.test` files; almost all are skipped ([SKIPS.md](../../../tests/mysql-test/SKIPS.md)).

Of the 327 `mysql-diff` steps, **81** run on both servers but skip row-text equality (`compare_output: false`) — typically `SHOW` / version / metadata that is allowed to differ.

### How to re-run

```bash
cargo test -- --skip release_binary
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs          # Docker MySQL 8.0 required
node scripts/mysql-gap-probe.mjs     # inventory; not a pass/fail gate
```

---

## 3. Progress (rusql vs MySQL over time)

| Date | `mysql-diff` vs Docker MySQL 8.0 | What it meant for users |
|------|----------------------------------|-------------------------|
| 2026-06-30 | **7/13** steps matched (54%) | Official CLI: UPDATE/DELETE errors, rows not visible across connections |
| 2026-07-03 | Same 7/13; protocol gaps (#73, #77) | Internal wire tests passed; **external MySQL client did not** |
| 2026-08-11 | **15/15**; CLI smoke 11/11 | Official client usable on a **small** subset; surface ~15–20% |
| **2026-09-20** | **297/297** compared | Official client matches MySQL on the **expanded** portable suite; 26 remaining rusql-only gaps in the probe |
| **2026-09-20 (M110)** | **313/313** compared | `TRUNCATE TABLE` added to the portable suite (heap delete-all + `AUTO_INCREMENT` reset) |
| **2026-09-20 (M111)** | **322/322** compared | `REPLACE INTO` added to the portable suite (PK delete-then-insert; SELECT compares `1,20`) |
| **2026-09-20 (M112)** | **327/327** compared | `INSERT IGNORE` added to the portable suite (skip PK conflict; SELECT compares `1,10` and `2,20`) |
| **2026-09-20 (M113)** | portable suite + functions | `SUBSTRING`/`SUBSTR`, `ROUND`, `DATE_ADD` added |
| **2026-09-20 (Phase Q exit)** | gap probe **19/29** remaining | M62–M113 complete; official MySQL CLI session introspection has no `unsupported function` |

The jump from 13 steps to 297 is **more tests on a larger subset**, plus real protocol/SQL work — not a claim that MySQL itself got smaller.

---

## 4. What works (matches MySQL on the tested subset)

Status **Works** means: accepted by rusql, and `mysql-diff` (or an equivalent wire test) expects MySQL-like results for the portable cases.

### Protocol and clients

| Capability | rusql | MySQL 8.0 | Evidence |
|------------|-------|-----------|----------|
| TCP handshake, `SELECT 1` | Works | Works | `mysql-diff` protocol smoke |
| Official `mysql` CLI | Works | Works | `mysql-diff` uses the same CLI on both |
| `caching_sha2_password` / `mysql_native_password` | Works (MVP) | Full plugin set | wire + auth tests |
| `COM_QUERY`, `USE` / `COM_INIT_DB` | Works | Works | CI |
| Binary prepared statements (`COM_STMT_*`) | Works (MVP) | Full | rusql wire tests (text `PREPARE` is **missing**) |
| `COM_CHANGE_USER` / `COM_RESET_CONNECTION` | Works | Works | M51 |
| `SHOW PROCESSLIST` | Works (registry) | Full processlist | M53; `information_schema.PROCESSLIST` is **missing** |

### Data definition and DML

| Capability | rusql | MySQL 8.0 |
|------------|-------|-----------|
| `CREATE`/`DROP DATABASE`, `USE` | Works | Works |
| `CREATE`/`DROP TABLE`, `CREATE INDEX` | Works | Works |
| `PRIMARY KEY`, `NOT NULL`, `UNIQUE` | Works | Works |
| `AUTO_INCREMENT` + `LAST_INSERT_ID()` (no-arg) | Works | Works |
| `ALTER TABLE` ADD/DROP/MODIFY/RENAME COLUMN, `RENAME TABLE` | Works | Broader ALTER |
| `FOREIGN KEY` + RESTRICT on DML | Works | Full referential actions |
| `INSERT` / `SELECT` / `UPDATE` / `DELETE` | Works | Works |
| `TRUNCATE TABLE` | Works (heap delete-all + AI reset; no DELETE triggers) | DDL truncate / tablespace reuse |
| `REPLACE INTO` | Works (single-column PK delete-then-insert; `affected_rows` 1 or 2) | Also UNIQUE-not-PK / multi-table REPLACE |
| `INSERT IGNORE` | Works (PK conflict skip; `affected_rows` = rows inserted) | Also UNIQUE-not-PK / sql_mode truncation IGNORE |
| `INSERT … SELECT`, `ON DUPLICATE KEY UPDATE` | Works (PK upsert) | Works |
| `CREATE TEMPORARY TABLE` | Works | Works |
| Types: `INT`, `VARCHAR`, `DECIMAL`, `DATETIME`, `TEXT`, `BLOB`, `JSON` (store) | Works | Full type system |

### Query SQL

| Capability | rusql | MySQL 8.0 |
|------------|-------|-----------|
| Projection, alias, `DISTINCT`, `ORDER BY`, `LIMIT`/`OFFSET` | Works | Works |
| `WHERE`: comparisons, `AND`/`OR`/`NOT`, `LIKE`, `BETWEEN`, `IN`, `IS NULL` | Works | Works |
| `INNER JOIN`, `LEFT`/`RIGHT OUTER JOIN` | Works | Works |
| `GROUP BY` / `HAVING` + `COUNT`/`SUM`/`MIN`/`MAX`/`AVG` | Works | Works |
| Subquery: `IN (SELECT)`, `EXISTS`, scalar, derived tables | Works | Broader |
| `UNION` / `UNION ALL` | Works | Also `INTERSECT`/`EXCEPT` |
| Non-recursive `WITH` CTE | Works | Also `WITH RECURSIVE` |
| Window: `ROW_NUMBER`/`RANK`/`DENSE_RANK` + `PARTITION BY`/`ORDER BY` | Works | Frames, more functions |
| `CASE` / `IF(cond, then, else)` | Works | Works |
| Views (`CREATE VIEW` + `SELECT`) | Works | `CREATE OR REPLACE VIEW` missing |

### Session, expressions, catalog (behavior that matched `mysql-diff`)

| Capability | Notes |
|------------|--------|
| `DATABASE()`/`SCHEMA()`, `USER()`/`CURRENT_USER()`, `VERSION()` | Session info |
| `CONNECTION_ID()`, `ROW_COUNT()`, `FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS` | Match tested cases |
| Arithmetic, `CONCAT`, `COALESCE`/`IFNULL`/`NULLIF`, `CAST`, `NOW`/`CURDATE`, `LENGTH`/`LOWER`/`UPPER`, `SUBSTRING`/`SUBSTR`, `ROUND`, `DATE_ADD` | Builtin pack |
| `utf8mb4_unicode_ci` / `utf8mb4_0900_ai_ci` compare/sort (sample corpus) | Not every collation |
| `EXPLAIN` + cost planner index/range paths | Shape, not InnoDB EXPLAIN |
| Transactions `BEGIN`/`COMMIT`/`ROLLBACK`; WAL survives restart | Snapshot isolation (MVCC) |
| `CREATE PROCEDURE`/`FUNCTION`/`TRIGGER`/`EVENT` (MVP) + `CALL` | Restricted dialect; see Partial |
| Event scheduler `AT` / `EVERY` / `STARTS`/`ENDS` / `DEFINER` / `ON COMPLETION` | Runs on next `COM_QUERY`, not a timer thread |

`mysql-diff` suites covering the above include `portable_dml`, `extended_where`, `outer_join`, `group_by_aggregate`, `subquery_*`, `union_queries`, `with_cte`, `window_functions`, `insert_select`, `on_duplicate_key_update`, `replace_into`, `insert_ignore`, `substring_round_date_add`, `foreign_key_restrict`, `alter_table_extended`, `auto_increment`, `last_insert_id`, `session_info`, `case_if`, event scheduler suites, and others listed in `crates/rusql-server/compat/mysql-diff.json`.

---

## 5. Partial: accepted, but not MySQL-equivalent

These often **succeed** so clients and ORMs can connect. Do not treat them as InnoDB.

| Feature | What rusql does | What MySQL 8.0 does |
|---------|-----------------|---------------------|
| `SHOW VARIABLES` / `@@` stubs | Small documented catalog; `SET SESSION` is in-memory | Hundreds of live variables |
| `SET GLOBAL` | Rejected (errno 1229) | Server-wide |
| `SET TRANSACTION ISOLATION LEVEL` | Overlays `@@transaction_isolation` only | Changes engine isolation |
| Engine isolation | Snapshot (MVCC) | InnoDB REPEATABLE READ + next-key locks |
| `SELECT … FOR UPDATE` / `FOR SHARE` / `SKIP LOCKED` | **No-op** — same rows, no wait | Row locks |
| `SHOW STATUS` / `SHOW TABLE STATUS` / `SHOW ENGINES` | Constant / stub cells | Live engine stats |
| `SHOW WARNINGS` / `SHOW ERRORS` | Empty list unless filled | Live diagnostics |
| `SHOW CREATE DATABASE` | Live per-schema charset (utf8mb4 collations) | Real charset per schema |
| `SHOW CREATE PROCEDURE`/`FUNCTION` | Reconstructs body; **empty parameter list** | Real params, DEFINER, sql_mode |
| `SHOW TRIGGERS` Definer / timestamps | Stubs (`root@%`, empty dates) | Live |
| `SHOW CREATE USER` | Plugin name, **no password hash** | Full `CREATE USER` dump |
| `GRANT`/`REVOKE` | MVP privilege checks | Full privilege tables |
| Procedures / functions | `BEGIN…END` MVP; no `IN`/`OUT`/`SIGNAL` | Full stored programs |
| Triggers | BEFORE INSERT; AFTER UPDATE/DELETE | All timings + more |
| Events | Catalog + COM_QUERY scheduler; COMMENT persisted; `information_schema.EVENTS` | Timer thread |
| `information_schema` | Virtual subset (`TABLES`, `COLUMNS`, `SCHEMATA`, `STATISTICS`, `ROUTINES`, `TRIGGERS`, `EVENTS`, …) | Full catalog |
| Binlog / replica | Row events on COMMIT; `COM_BINLOG_DUMP` follow; GTID **stub** | Production replication + GTID failover |
| `VERSION()` handshake | `8.0.33-rusql` | Oracle version string |
| JSON type | Stored; **`JSON_EXTRACT` missing** | Full JSON functions |
| `MONTH`/`YEAR` event intervals | 30/365-day approximation | Calendar months/years |

---

## 6. What does not work (failed vs MySQL or unimplemented)

From the **2026-09-20 post-M113 gap probe**: 29 probes, **19 rusql gaps**, 9 ok (including M108–M113 plus `CREATE TEMPORARY TABLE` and `UNIQUE`), 1 both-fail (`CREATE PROCEDURE … IN` because of `DELIMITER` / dialect). Phase Q session-introspection exit is met (official `mysql:8.0` CLI: `DATABASE`/`USER`/`VERSION`/`CONNECTION_ID`/`@@`/`SHOW VARIABLES`/`SET NAMES` have no `unsupported function`). `information_schema.EVENTS` is queryable (not errno 1146) but an empty catalog can `shape_mismatch` vs MySQL batch headers.

### Filed Phase Q issues (complete)

| SQL / feature | rusql | MySQL 8.0 | Issue |
|---------------|-------|-----------|-------|
| `CREATE EVENT … COMMENT '…'` | Done (M108) | Persists comment | [M108 #250](https://github.com/tanbamboo/rusql/issues/250) |
| `information_schema.EVENTS` | Done (M109) | Catalog view | [M109 #251](https://github.com/tanbamboo/rusql/issues/251) |
| `TRUNCATE TABLE` | Done (M110) | Heap delete-all + `AUTO_INCREMENT` reset | [M110 #252](https://github.com/tanbamboo/rusql/issues/252) |
| `REPLACE INTO` | Done (M111) | Delete+insert on single-column PK | [M111 #253](https://github.com/tanbamboo/rusql/issues/253) |
| `INSERT IGNORE` | Done (M112) | Skip PK conflicts; insert the rest | [M112 #254](https://github.com/tanbamboo/rusql/issues/254) |
| `SUBSTRING` / `ROUND` / `DATE_ADD` | Done (M113) | Builtins | [M113 #255](https://github.com/tanbamboo/rusql/issues/255) |

### Phase R (in progress)

| SQL / feature | rusql | MySQL 8.0 | Issue |
|---------------|-------|-----------|-------|
| `CREATE DATABASE … CHARACTER SET … COLLATE …` | Done (M114) | Schema charset | [M114 #265](https://github.com/tanbamboo/rusql/issues/265) |

### Post-Q probe gaps (Phase R filed)

| SQL / feature | Typical production impact | Issue |
|---------------|---------------------------|-------|
| `JSON_EXTRACT(...)` | JSON columns in apps | [M115 #266](https://github.com/tanbamboo/rusql/issues/266) |
| `UUID()` | Generated identifiers | [M116 #267](https://github.com/tanbamboo/rusql/issues/267) |
| `LAST_INSERT_ID(expr)` | Sequence helpers | [M117 #268](https://github.com/tanbamboo/rusql/issues/268) |
| `GET_LOCK(...)` | App-level advisory locks | [M118 #269](https://github.com/tanbamboo/rusql/issues/269) |
| `information_schema.TABLE_CONSTRAINTS` | ORM / migrator introspection | [M119 #270](https://github.com/tanbamboo/rusql/issues/270) |
| `information_schema.PROCESSLIST` | Monitoring | [M120 #271](https://github.com/tanbamboo/rusql/issues/271) |
| `information_schema.PARAMETERS` | Stored program metadata | [M121 #272](https://github.com/tanbamboo/rusql/issues/272) |
| `SHOW BINARY LOGS` | Replication ops | [M122 #273](https://github.com/tanbamboo/rusql/issues/273) |
| `SHOW BINLOG EVENTS` | Replication ops | [M123 #274](https://github.com/tanbamboo/rusql/issues/274) |
| `CREATE OR REPLACE VIEW` | View deploy | [M124 #275](https://github.com/tanbamboo/rusql/issues/275) |
| `PREPARE` / `EXECUTE` (text SQL) | Many clients; binary `COM_STMT_*` exists | [M125 #276](https://github.com/tanbamboo/rusql/issues/276) |
| `SAVEPOINT` | Nested rollback | [M126 #277](https://github.com/tanbamboo/rusql/issues/277) |
| `WITH RECURSIVE` | Hierarchical queries | [M127 #278](https://github.com/tanbamboo/rusql/issues/278) |
| `INTERSECT` | Set SQL | [M128 #279](https://github.com/tanbamboo/rusql/issues/279) |
| Window `ROWS BETWEEN …` frames | Analytics SQL | [M129 #280](https://github.com/tanbamboo/rusql/issues/280) |
| `CREATE EVENT … DISABLE ON SLAVE` | Replica event control | [M130 #281](https://github.com/tanbamboo/rusql/issues/281) |
| `SHOW ENGINE INNODB STATUS` | DBA ops | [M131 #282](https://github.com/tanbamboo/rusql/issues/282) |
| `CREATE PROCEDURE … IN` | Stored program params | [M132 #283](https://github.com/tanbamboo/rusql/issues/283) |

### Larger MySQL surfaces rusql does not claim

- Full InnoDB (tablespaces, crash recovery identical to Oracle, XA, gap locks)
- Partitioning, generated columns, check constraints beyond current FK
- Full `performance_schema`, plugins, UDF .so
- GIS, full-text, spatial indexes
- Role graphs, TLS, enterprise auth plugins
- Group Replication / InnoDB Cluster / clone
- Cost-based optimizer feature-complete with MySQL
- Restoring a generic `mysqldump` of an existing production schema

---

## 7. SQL functions cheat sheet

| Function / expression | rusql | MySQL 8.0 |
|-----------------------|-------|-----------|
| `CONCAT`, `LENGTH`, `LOWER`, `UPPER` | Works | Works |
| `COALESCE`, `IFNULL`, `NULLIF`, `CAST` | Works | Works |
| `NOW`, `CURDATE` | Works | Broader date API |
| `CASE`, `IF(...)` | Works | Works |
| `COUNT`/`SUM`/`MIN`/`MAX`/`AVG` | Works | Works |
| `DATABASE`, `USER`, `VERSION`, `CONNECTION_ID`, `ROW_COUNT` | Works | Works |
| `LAST_INSERT_ID()` | Works | Works |
| `LAST_INSERT_ID(expr)` | **Missing** | Works |
| `FOUND_ROWS` | Works | Deprecated in 8.0.17+ but present |
| `ROW_NUMBER`/`RANK`/`DENSE_RANK` | Works (no frames) | Frames + more windows |
| `SUBSTRING`, `ROUND`, `DATE_ADD` | Works (M113: 1-based substring; half-away-from-zero `ROUND`; `DATE_ADD` INTERVAL; `MONTH`/`YEAR` 30/365-day) | Works |
| `JSON_EXTRACT`, `UUID`, `GET_LOCK` | **Missing** | Works |

---

## 8. Performance (not a parity score)

Throughput numbers are **not** a substitute for SQL coverage. August 2026 single-connection CLI loop ([details](performance-benchmark-2026-08-11.md)):

| Workload | rusql / MySQL QPS (then) |
|----------|--------------------------|
| `SELECT 1` | 1.55× |
| PK point select | 0.92× |
| Index lookup | 0.91× |
| Scan + `ORDER BY` + `LIMIT` | 0.74× |
| Single `INSERT` | 1.96× |
| PK `UPDATE` | 0.62× |
| `BEGIN`+`INSERT`+`COMMIT` | 0.92× |

Those runs include CLI spawn overhead. Sysbench `oltp_point_select` is the industry-shaped read gate (`scripts/sysbench-rusql.mjs`, default rusql ≥ 0.7× MySQL when the tools exist). Neither benchmark expands the SQL surface.

---

## 9. Decision guide

| Your situation | Use rusql? |
|----------------|------------|
| Learning MySQL protocol / contributing to rusql | **Yes** |
| New app using only the **Works** tables above, accepting snapshot isolation | **Maybe** (dev / non-critical) |
| Existing MySQL app, unknown SQL, dumps, ORMs with migrations | **Not yet** |
| Need JSON extract / advisory locks | **Not yet** (backlog) |
| Need InnoDB locking, XA, GTID failover, ops `SHOW ENGINE` | **No** |
| Production data, compliance, multi-AZ HA | **No** — use MySQL 8.0 or a production-grade fork |

When in doubt, run your own SQL against both:

```bash
# rusql
mysql -h 127.0.0.1 -P 3307 -u root --protocol=TCP

# compare the same statements via
node scripts/mysql-diff.mjs
node scripts/mysql-gap-probe.mjs
```

---

## 10. Related docs

| Doc | Role |
|-----|------|
| [User guide](../user-guide.md) | How to run and verify features |
| [Full parity roadmap](../specs/mysql-full-parity-roadmap.md) | Milestone backlog (M108+) |
| [Release notes](../release-notes.md) | What landed on `main` |
| [mysql-test SKIPS](../../../tests/mysql-test/SKIPS.md) | Why Oracle MTR is not the gate |

*Update this report when `mysql-diff` step counts, gap-probe results, or the production verdict change.*

# Performance Benchmark Baseline — rusql vs MySQL 8.0

**Date**: 2026-08-11  
**Purpose**: Establish a reproducible performance baseline for the rusql optimization phase.  
**Scope**: Supported SQL subset only; not a claim of full MySQL feature parity.

---

## 1. Feature parity summary

**rusql is not functionally identical to MySQL 8.0.** The project targets a *credible wire + SQL subset* for real clients (CLI, drivers, ORMs), not full Oracle MySQL surface coverage.

| Dimension | MySQL 8.0 | rusql (2026-08-11 `main`) |
|-----------|-----------|---------------------------|
| Estimated SQL / protocol surface | 100% | ~5–20% (roadmap estimate; growing by milestone) |
| Official `mysql-test` corpus | ~thousands of cases | 20-case wire subset; vast majority skipped ([SKIPS.md](../../tests/mysql-test/SKIPS.md)) |
| Differential gate (`mysql-diff`) | N/A | 15/15 portable steps pass vs Docker MySQL 8.0 |
| Third-party CLI smoke (Aug 2026) | N/A | 11/11 matrix + 8/8 `mysql_cli` tests pass |

### Implemented (high level)

- Wire handshake, COM_QUERY, COM_STMT_* prepared statements, COM_INIT_DB / USE, COM_PING
- DML/DDL subset: CREATE TABLE, indexes, INSERT/SELECT/UPDATE/DELETE, ALTER ADD COLUMN, views (M33)
- Transactions: BEGIN/COMMIT/ROLLBACK, durable WAL (M31), MVCC snapshot isolation (M55)
- Metadata: SHOW/DESCRIBE, information_schema, SHOW CREATE TABLE, SHOW INDEX
- Auth: caching_sha2 (+ RSA path), utf8mb4 charset metadata (M35)
- Binlog: QUERY_EVENT spike only (M34), not production replication

### Major remaining gaps

| Category | Examples |
|----------|----------|
| SQL surface | No stored procedures/triggers/UDF; limited types; no AUTO_INCREMENT; many statements return "unsupported" |
| DDL / catalog | No `CREATE DATABASE`; limited ALTER; partial privilege model |
| Optimizer | No cost-based planner, join cache, range analysis, subquery decorrelation |
| Replication | No GTID, semi-sync, or full binlog/replica pipeline |
| mysql-test | Replication (`rpl-*`), charset suites, optimizer suites, multi-connection scripts skipped |
| Protocol edge cases | Ongoing gaps tracked via GitHub issues (e.g. session track, metadata packets) |

**Conclusion**: Compatibility testing (Aug 2026) shows rusql works well for its *supported subset* against the official MySQL 8.0 client, but feature parity is far from complete.

---

## 2. Industry benchmark methods (survey)

| Tool | Role | Applicability to rusql today |
|------|------|------------------------------|
| **[Sysbench](https://github.com/akopytov/sysbench)** | De facto MySQL OLTP micro-benchmark (`oltp_read_only`, `oltp_write_only`, `oltp_read_write`) | Partially applicable; many Sysbench tables/queries exceed current SQL support |
| **mysqlslap** | Built-in MySQL load generator | Not shipped in official `mysql:8.0` Docker image used here |
| **TPC-C / TPC-H** | Standard OLTP/OLAP benchmarks | Too broad; schema and query mix unsupported |
| **mysql-test (MTR)** | Correctness, not throughput | Used for functional compat, not perf |
| **Percona Benchmark Suite** | Packaging around Sysbench + tooling | Same SQL surface constraints as Sysbench |

**Chosen approach**: Custom single-connection micro-benchmark driven by the **official MySQL 8.0 CLI** (same client for both servers), exercising only SQL that rusql supports today. Full Sysbench/TPC suites are deferred until schema and statement coverage expand.

---

## 3. Test environment

| Item | Value |
|------|-------|
| Host OS | Windows 10 (build 26200) |
| rusql | `target/release/rusql-server.exe`, port **3307**, data dir `.test-data-bench-20260811` |
| MySQL | Docker `mysql:8.0` container `rusql-mysql80-bench`, port **3308** |
| Client | `mysql:8.0` CLI inside Docker → `host.docker.internal` |
| Schema | `bench_t (id INT PK, k INT, name VARCHAR(32))`, index on `k`, **10,000** seed rows |
| Warmup | 20 iterations per workload |
| Concurrency | **1** (new CLI invocation per query — includes process + TCP overhead) |

### Workloads

| Name | SQL pattern | Iterations |
|------|-------------|------------|
| `select1` | `SELECT 1` | 1000 |
| `point_select_pk` | `SELECT name FROM bench_t WHERE id = 5000` | 1000 |
| `index_lookup` | `SELECT id FROM bench_t WHERE k = 42` | 1000 |
| `scan_order_limit` | `SELECT id FROM bench_t ORDER BY k LIMIT 100` | 1000 |
| `insert_single` | `INSERT INTO bench_t (id,k,name) VALUES (…)` sequential IDs | 500 |
| `update_pk` | `UPDATE bench_t SET name='u' WHERE id = 5000` | 500 |
| `begin_commit` | `BEGIN; INSERT …; COMMIT` | 200 |

Raw JSON artifacts (local, not committed): `.bench-rusql.json`, `.bench-mysql-writes.json` (MySQL read phase in `.bench-mysql.json`).

---

## 4. Results

### 4.1 Throughput (QPS, higher is better)

| Workload | rusql :3307 | MySQL :3308 | rusql / MySQL |
|----------|-------------|-------------|---------------|
| SELECT 1 | **67.97** | 43.92 | **1.55×** |
| Point SELECT (PK) | 52.53 | **57.01** | 0.92× |
| Index lookup | 42.76 | **47.04** | 0.91× |
| Scan + ORDER BY + LIMIT 100 | 35.93 | **48.43** | 0.74× |
| INSERT single | **58.36** | 29.71 | **1.96×** |
| UPDATE (PK) | 34.76 | **55.77** | 0.62× |
| BEGIN + INSERT + COMMIT | 34.00 | **37.00** | 0.92× |

### 4.2 Latency (avg ms per operation, lower is better)

| Workload | rusql | MySQL |
|----------|-------|-------|
| SELECT 1 | 14.7 | 22.8 |
| Point SELECT (PK) | 19.0 | 17.5 |
| Index lookup | 23.4 | 21.3 |
| Scan + ORDER BY + LIMIT 100 | 27.8 | 20.6 |
| INSERT single | 17.1 | 33.7 |
| UPDATE (PK) | 28.8 | 17.9 |
| BEGIN + INSERT + COMMIT | 29.4 | 27.0 |

---

## 5. Interpretation and optimization targets

### Caveats (read before using as SLO)

1. **Single-threaded CLI loop** — Dominant cost is often process spawn + TCP round-trip, not query execution. Absolute QPS is low for both engines.
2. **Docker networking** — Client runs in container; server on host (`host.docker.internal`). Adds latency vs co-located Sysbench.
3. **Unequal optimization maturity** — MySQL has decades of buffer pool, redo log, and executor tuning; rusql is MVP storage + executor.
4. **Small working set** — 10k rows fit easily in memory on both sides; not a disk I/O stress test.

### Baseline findings

| Area | Observation | Suggested optimization phase focus |
|------|-------------|-----------------------------------|
| Simple protocol path | rusql faster on `SELECT 1` and single INSERT | Likely lower fixed overhead; validate with persistent connection benchmark |
| Primary / secondary read | Within ~10% on point/index reads | Acceptable; prioritize scan+sort path |
| **Scan + ORDER BY + LIMIT** | rusql **26% slower** | Sort implementation, limit pushdown, allocation |
| **UPDATE by PK** | rusql **38% slower** | WAL fsync policy, row update path, lock/MVCC overhead |
| Transactions | Near parity on small txn mix | Good starting point; scale with multi-row and contention tests |

### Recommended next benchmarks (optimization phase)

1. **Persistent connection driver** — Same workloads via `mysql2`/Rust client (remove CLI spawn noise).
2. **Sysbench `oltp_point_select` only** — After table DDL compatibility confirmed.
3. **Multi-thread** — 1/4/8/16 clients; identify lock and connection bottlenecks.
4. **Durability modes** — rusql WAL sync policy vs MySQL `innodb_flush_log_at_trx_commit`.
5. **Larger data** — 1M+ rows to expose scan and I/O behavior.

---

## 6. Reproduction (abbreviated)

```bash
# rusql
cargo build --release -p rusql-server
./target/release/rusql-server --port 3307 --data-dir ./.test-data-bench

# MySQL 8.0 (Docker)
docker run -d --name rusql-mysql80-bench -e MYSQL_ALLOW_EMPTY_PASSWORD=yes -p 3308:3306 mysql:8.0

# Run benchmark script (Linux container or WSL); see session artifact rusql-bench.sh
```

Re-run sensors after any harness change: `cargo test -p rusql-server mysql_cli` and `node scripts/mysql-diff.mjs`.

---

## 7. References

- [MySQL full parity roadmap (M36+)](../specs/mysql-full-parity-roadmap.md) — GitHub issues #100–#131
- [MySQL compat roadmap](../specs/mysql-compat-roadmap.md)
- [Functional test report (2026-07-03)](functional-test-report-2026-07-03.md)
- [mysql-test SKIPS](../../tests/mysql-test/SKIPS.md)
- Oracle Sysbench documentation: https://dev.mysql.com/doc/refman/8.0/en/sysbench.html

---

## 8. Multi-threaded concurrency (PERF-B4)

Run with `node scripts/bench-rusql-vs-mysql.mjs --thread-matrix --compare`.

| Threads | rusql total QPS | MySQL total QPS | rusql/MySQL | Scaling note |
|---------|-----------------|-----------------|-------------|--------------|
| 1 | baseline | baseline | ~1.0× | Single-writer storage |
| 4 | TBD | TBD | TBD | Lock contention expected |
| 8 | TBD | TBD | TBD | Sub-linear if storage serializes writes |
| 16 | TBD | TBD | TBD | Identify bottleneck (WAL, RwLock, executor) |

**Interpretation**: rusql uses single-writer persistent storage with MVCC snapshot reads. Expect read-heavy workloads to scale better than write-heavy mixes. Sub-linear scaling at 8+ threads indicates lock or WAL contention — see PERF-B5 for WAL tuning.

---

## 9. WAL sync policy matrix (PERF-B5)

Server flag: `--wal-sync=always|batch|none` (default `always`).

| Policy | `fsync` on autocommit | `fsync` on txn commit | MySQL equivalent | Data-loss risk |
|--------|----------------------|----------------------|------------------|----------------|
| `always` | yes | yes | `innodb_flush_log_at_trx_commit=1` | none (default) |
| `batch` | no | yes | `innodb_flush_log_at_trx_commit=2` | crash may lose last autocommit |
| `none` | no | no | `innodb_flush_log_at_trx_commit=0` | crash may lose recent writes |

Benchmark `begin_commit` workload with each policy:

```bash
for mode in always batch none; do
  cargo run -p rusql-server -- --wal-sync $mode --port 3307 --data-dir ./.test-data-bench &
  sleep 2
  node scripts/bench-rusql-vs-mysql.mjs --host 127.0.0.1 --port 3307 --workloads begin_commit --label $mode
  kill %1
done
```

---

## 10. Sysbench oltp_point_select (PERF-B6)

Industry-standard OLTP read benchmark via `scripts/sysbench-rusql.mjs`.

**Prerequisites**: Sysbench installed, Docker MySQL 8.0 on port 3308, rusql on 3307.

```bash
node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308 --threads 8 --time 30 --threshold 0.7
```

- Soft-fails (exit 0) when Sysbench or Docker unavailable
- Fails when rusql QPS < 70% of MySQL (configurable `--threshold`)
- CI: `.github/workflows/sysbench.yml` (`workflow_dispatch` only — does not block PR CI)

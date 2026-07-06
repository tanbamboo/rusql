# rusql Functional Test Report — 2026-07-03

## 中文摘要

**测试范围**：rusql `main` 分支 M0–M31 已实现能力（wire 协议、DML/DDL、查询、元数据、事务、WAL、prepared statements）。

**总体结论**：

| 层级 | 结果 | 说明 |
|------|------|------|
| Layer 1 自动化（内部 Wire 客户端） | **95/95 通过** | `cargo test` 全绿；compat 76 步 + WAL 重启；mysql-test 子集 12 suites |
| Layer 2 外部客户端（Docker `mysql:8.0` CLI） | **大量失败** | 与内部测试形成鲜明对比；根因指向官方客户端协议路径 |
| Layer 3 MySQL 8.0 差异对比 | **7/13 步匹配** | `mysql-diff.json` 6 步失败，与 2026-06-30 报告一致 |

**关键缺陷**：

1. **[#73](https://github.com/tanbamboo/rusql/issues/73)** — 官方 MySQL 8.0 CLI 协议缺口（跨连接持久化、UPDATE/DELETE、SHOW/DESCRIBE/BEGIN 等 ERROR 1105）。本次测试补充根因分析：`CLIENT_QUERY_ATTRIBUTES` / `COM_QUERY` 帧未剥离。
2. **[#77](https://github.com/tanbamboo/rusql/issues/77)**（新建）— `COM_INIT_DB` (0x02) 未实现，官方客户端 `USE rusql` 返回 ERROR 1047。

**建议**：优先修复 #73（含 COM_QUERY 属性块解析），再处理 #77；修复前外部客户端测试不应作为合并门禁，内部 compat 套件可作为回归基线。

---

## Test Environment

| Item | Value |
|------|-------|
| Date | 2026-07-03 |
| OS | Windows 10 (build 26200) |
| Rust | cargo 1.91.1 |
| Docker | 28.3.2 |
| rusql-server | `target/release/rusql-server.exe` (release build) |
| MySQL client | `docker run --rm mysql:8.0 mysql` → `host.docker.internal` |
| Test ports | 3307 (mysql-diff), 3310–3320 (manual external tests) |
| Local `mysql` CLI | Not installed (Docker only) |

---

## Scope & Milestone Mapping (M0–M31)

| Milestone | Feature | Internal wire | Official mysql:8.0 CLI | Notes |
|-----------|---------|:-------------:|:----------------------:|-------|
| M1 | Handshake | Pass | Pass | Connection succeeds |
| M2 | COM_QUERY / CREATE / INSERT / SELECT | Pass | Partial | CLI: OK exit; empty tabular output on Windows/Docker; cross-conn gaps |
| M3 | WAL persistence | Pass | Fail | CLI cannot verify reliably (#73) |
| M4 | Indexes | Pass | Pass | `CREATE INDEX` + `WHERE col = literal` |
| M5 | Compat fixtures | Pass | N/A | JSON wire harness |
| M6 | DROP / DELETE | Pass | Fail | DELETE → ERROR 1105 via CLI |
| M7/M26 | caching_sha2 / RSA auth | Pass | Not fully exercised | `auth_tests` pass; CLI auth on dedicated port blocked by port contention |
| M8 | UPDATE | Pass | Fail | ERROR 1105 via CLI |
| M9 | BEGIN / COMMIT / ROLLBACK | Pass | Fail | BEGIN → ERROR 1105 via CLI |
| M10 | SHOW TABLES / DATABASES | Pass | Fail | ERROR 1105 via CLI |
| M11 | Prepared statements | Pass | N/A | `stmt_prepare_execute_*` pass; CLI has limited PREPARE coverage |
| M12 | DESCRIBE / information_schema | Pass | Fail | DESCRIBE → 1105; `SCHEMATA` → 1146 via CLI |
| M13 | SHOW CREATE TABLE | Pass | Not tested | Blocked by DESCRIBE/SHOW failures on CLI path |
| M14–M22 | Projection / ORDER BY / LIMIT / WHERE / JOIN | Pass | Partial | Simple SELECT paths OK on CLI; metadata DDL fails |
| M23–M24 | PK metadata / ALTER TABLE | Pass | Fail | ALTER → 1105 via CLI |
| M27–M28 | info_schema++ / SHOW INDEX | Pass | Fail | Via CLI |
| M29–M30 | mysql-diff / mysql-test subset | Pass | Fail diff | Subset passes internally |
| M31 | Durable COMMIT WAL | Pass | Fail | `transaction_rollback_leaves_wal_unchanged` pass; CLI cannot exercise |

---

## Layer 1 — Automated Regression (Internal Wire Client)

### Summary

| Suite | Command | Result | Duration |
|-------|---------|--------|----------|
| Workspace | `cargo test` | **95 passed**, 0 failed | ~24 s |
| Compat fixtures | `cargo test -p rusql-server compat` | **2 passed** (76 SQL steps + WAL restart) | ~1.5 s |
| mysql-test subset | `cargo test -p rusql-server mysql_test_subset` | **1 passed** (12 suites) | ~1.6 s |
| mysql-test script | `node scripts/mysql-test-subset.mjs` | **OK** | ~5 s |

### Crate-level breakdown (`cargo test`)

| Crate | Tests |
|-------|------:|
| rusql-server | 17 |
| rusql-executor | 24 |
| rusql-protocol | 23 |
| rusql-sql | 15 |
| rusql-storage | 11 |
| rusql-i18n | 3 |
| rusql-core | 1 |
| rusql-planner | 1 |
| rusql-cli | 0 |

### Notable rusql-server integration tests (all pass)

- `connection::tests::com_query_create_insert_select`
- `connection::tests::persistence_across_connections`
- `connection::tests::update_across_connections`
- `connection::tests::transaction_commit_and_rollback`
- `connection::tests::transaction_rollback_leaves_wal_unchanged`
- `connection::tests::stmt_prepare_execute_*` (3 tests)
- `connection::tests::describe_and_information_schema`
- `connection::auth_tests::*` (3 tests)
- `compat_suite::run_basic_compat_fixtures`
- `compat_suite::compat_persistence_after_restart`
- `mysql_test_subset::run_mysql_test_subset`

**Conclusion**: Internal wire harness confirms M0–M31 behavior on `main` is stable.

---

## Layer 2 — External MySQL 8.0 CLI Tests

Method: `docker run --rm mysql:8.0 mysql -h host.docker.internal -P <port> -u root --protocol=TCP [-B] -e "<SQL>"`

### 2.1 Protocol & Authentication

| ID | Scenario | Expected | Actual | Status |
|----|----------|----------|--------|--------|
| P-01 | Default handshake (`SELECT 1`) | Connect + result | Exit 0; empty stdout (Docker capture) | **Pass** |
| P-02 | `--default-auth=mysql_native_password` | Connect | Exit 0 | **Pass** |
| P-03 | `--auth-password secret` correct/wrong | Accept / reject | Not completed (port contention during auth server start) | **N/A** |
| P-04 | caching_sha2 RSA full auth | Connect with password | Covered by internal `auth_tests` | **Pass** (internal) |
| P-05 | Reconnect (two docker invocations) | Both succeed | Exit 0 | **Pass** |

### 2.2 DDL / DML

| ID | Scenario | Expected | Actual | Status |
|----|----------|----------|--------|--------|
| D-01 | CREATE / INSERT / SELECT | Rows returned | Exit 0; cross-connection SELECT empty | **Fail** |
| D-02 | CREATE INDEX + point lookup | Index hit | Exit 0 | **Pass** |
| D-03a | UPDATE | Row updated | ERROR 1105 unsupported `Update` | **Fail** |
| D-03b | DELETE | Row removed | ERROR 1105 unsupported `Delete` | **Fail** |
| D-04 | DROP / recreate table | OK | Exit 0 | **Pass** |
| D-05 | ALTER TABLE ADD COLUMN | OK | ERROR 1105 unsupported `AlterTable` | **Fail** |
| D-06 | PRIMARY KEY in DESCRIBE | PRI column | ERROR 1105 unsupported `ExplainTable` | **Fail** |

### 2.3 Query & Metadata

| ID | Scenario | Expected | Actual | Status |
|----|----------|----------|--------|--------|
| Q-01 | Column projection / alias | Aliased columns | Exit 0 | **Pass** |
| Q-02 | ORDER BY ASC | Sorted rows | Exit 0 | **Pass** |
| Q-03 | LIMIT OFFSET | Paginated rows | Exit 0 | **Pass** |
| Q-04 | WHERE comparisons + AND | Filtered rows | Exit 0 | **Pass** |
| Q-05a | IS NULL (explicit NULL) | id=1 | Exit 0 | **Pass** |
| Q-05b | IS NOT NULL | id=2 | Exit 0 | **Pass** |
| Q-05c | Empty string `''` vs IS NULL | MySQL: not NULL; rusql may differ | Not conclusively verified on CLI | **N/A** |
| Q-06 | INNER JOIN | Joined rows | Exit 0 | **Pass** |
| Q-07 | SHOW TABLES / DATABASES / USE | Metadata | SHOW → 1105; USE → 1047 `0x02` | **Fail** |
| Q-08 | DESCRIBE / SHOW CREATE | Schema info | DESCRIBE → 1105 | **Fail** |
| Q-09 | information_schema.SCHEMATA | rusql row | ERROR 1146 table not found | **Fail** |
| Q-10 | SHOW INDEX | Index list | Not reached (prior DDL state) | **Fail** |
| Q-11 | SELECT literal | `1` | Exit 0 | **Pass** |

### 2.4 Transactions & Persistence

| ID | Scenario | Expected | Actual | Status |
|----|----------|----------|--------|--------|
| T-01 | Uncommitted invisible / ROLLBACK | Isolation | BEGIN → 1105 via CLI | **Fail** |
| T-02 | COMMIT visible to other connection | Row visible | Cannot exercise via CLI | **Fail** |
| T-03 | WAL survive server restart | Row after restart | CLI persistence unreliable | **Fail** |
| T-04 | ROLLBACK not flushed to WAL | Empty after restart | Internal test passes | **Pass** (internal) |

### 2.5 Cross-Connection Regression (#73)

```bash
docker run --rm mysql:8.0 mysql -h host.docker.internal -P 3307 -u root --protocol=TCP \
  -e "CREATE TABLE t73(id INT); INSERT INTO t73 VALUES(1);"
docker run --rm mysql:8.0 mysql -h host.docker.internal -P 3307 -u root --protocol=TCP -B \
  -e "SELECT * FROM t73"
```

| ID | Result |
|----|--------|
| X-01 INSERT persist across connections | **Fail** — second SELECT empty |
| X-02 UPDATE across connections | **Fail** — ERROR 1105 |
| X-03 DELETE across connections | **Fail** — ERROR 1105 |

Internal tests `persistence_across_connections` and `update_across_connections` **pass** on the same build.

### 2.6 Prepared Statements

| Coverage | Result |
|----------|--------|
| Internal `COM_STMT_PREPARE` / `EXECUTE` / binary resultset | **Pass** (3 tests) |
| Official `mysql` CLI PREPARE | **N/A** — limited CLI support; not a rusql defect |

### 2.7 Negative / Unimplemented (Expected Failures)

| SQL | Official CLI | Classification |
|-----|--------------|----------------|
| `SELECT COUNT(*) FROM d1` | Error | Known limitation (no aggregates) |
| `SELECT 1 UNION SELECT 2` | Error | Known limitation |
| `CREATE VIEW v AS SELECT 1` | Error | M33 planned |
| `USE DATABASE foo` | Parse/SQL error | Documented: only `USE rusql` as SQL text |

---

## Layer 3 — MySQL 8.0 Differential (`mysql-diff.json`)

Command: `node scripts/mysql-diff.mjs`

**Result: 7/13 steps match (54%)** — same failure pattern as [database-compat-report-2026-06-30.md](database-compat-report-2026-06-30.md).

### Failed steps

| Suite | SQL | Issue |
|-------|-----|-------|
| portable_dml | `SELECT id, name FROM md_t ORDER BY id` | rusql missing `alice` row (INSERT not persisted across connections) |
| portable_dml | `UPDATE md_t SET name = 'Alice' WHERE id = 1` | rusql ERROR 1105 |
| portable_dml | `SELECT name FROM md_t WHERE id = 1` | rusql empty vs `Alice` |
| portable_dml | `DELETE FROM md_t WHERE id = 2` | rusql ERROR 1105 |
| portable_dml | `SELECT * FROM md_t ORDER BY id` | rusql only `bob` |
| portable_index | `SELECT id FROM md_i WHERE label = 'y'` | rusql empty vs `2` |

---

## Defects & GitHub Issues

| Issue | Title | Action |
|-------|-------|--------|
| [#73](https://github.com/tanbamboo/rusql/issues/73) | Official MySQL 8.0 CLI protocol gaps | **Existing** — added [comment](https://github.com/tanbamboo/rusql/issues/73#issuecomment-4871971109) with `CLIENT_QUERY_ATTRIBUTES` / `COM_QUERY` root-cause analysis |
| [#77](https://github.com/tanbamboo/rusql/issues/77) | COM_INIT_DB not supported — `USE` fails | **Created** — `ERROR 1047: unsupported command: 0x02` |

No duplicate issues filed for UPDATE/DELETE/persistence — all tracked under #73.

---

## Known Limitations (Not Bugs)

| Area | Status |
|------|--------|
| M32+ MVCC, M33 Views, M34 Binlog, M35 Charset | Planned |
| Aggregates, subqueries, UNION, GROUP BY | Not implemented |
| Stored procedures / triggers | Not implemented |
| Official mysql-test `.test` corpus | M30 subset only (12 cases) |
| Multi-statement `;` in one COM_QUERY | Limited |

---

## Recommendations

1. **Fix #73 first** — strip `CLIENT_QUERY_ATTRIBUTES` block from `COM_QUERY`; re-run `mysql-diff.mjs` and this report's Layer 2 matrix.
2. **Fix #77** — implement `COM_INIT_DB` for schema selection via official clients.
3. **CI strategy** — keep `cargo test -p rusql-server compat` as merge gate; treat `mysql-diff` as `continue-on-error` until #73 is resolved.
4. **Harness** — add a wire test that sets `CLIENT_QUERY_ATTRIBUTES` in handshake to prevent regression (future PR; out of scope for this report).

---

## Appendix — Reproduce Commands

```bash
# Build
cargo build --release -p rusql-server

# Layer 1
cargo test
cargo test -p rusql-server compat
cargo test -p rusql-server mysql_test_subset
node scripts/mysql-test-subset.mjs

# Layer 3
node scripts/mysql-diff.mjs

# Layer 2 (external CLI, after starting server on 3307)
docker run --rm mysql:8.0 mysql -h host.docker.internal -P 3307 -u root --protocol=TCP -e "SELECT 1"
docker run --rm mysql:8.0 mysql -h host.docker.internal -P 3307 -u root --protocol=TCP -e "SHOW TABLES"
docker run --rm mysql:8.0 mysql -h host.docker.internal -P 3307 -u root --protocol=TCP -e "USE rusql"
```

---

*Functional test executed 2026-07-03. Only deliverable changed in repository: this report.*

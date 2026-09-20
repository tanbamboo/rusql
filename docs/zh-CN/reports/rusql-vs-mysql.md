# rusql 与 MySQL 8.0 — 兼容性测试报告

**截止日期：** 2026-09-20（`main` 在 M113 / Phase Q 完成之后）  
**读者：** 想知道「能不能把 rusql 当 MySQL 用」的用户  
**English:** [rusql-vs-mysql.md](../../en/reports/rusql-vs-mysql.md)

这是当前用户向对比。历史快照：[2026-06](../../en/reports/database-compat-report-2026-06-30.md)、[2026-07](../../en/reports/functional-test-report-2026-07-03.md)、[2026-08 性能](performance-benchmark-2026-08-11.md)。

---

## 1. 结论：能不能上生产？

**不能把 rusql 当作 MySQL 8.0 的即插即用替代。**

rusql 使用 MySQL 线协议，并且在当前差异测试套件里，每条可移植语句都与 Docker MySQL 8.0 对齐。这是真实进展。它仍是**不断扩展的子集**，不是完整引擎：许多日常语句会报错，部分 `SHOW` 目录是桩数据，`FOR UPDATE` 不加锁，复制只是 MVP 转储路径，不是故障转移。

| 问题 | 答案 |
|------|------|
| 官方 `mysql` CLI 能否连接并做 CRUD？ | **能**，限于已支持子集 |
| JDBC / 常见连接器能否握手？ | **多数可以**（有会话 `@@` 桩值） |
| 现成生产库 + ORM 能否原样迁移？ | **不能** — 缺 JSON 提取、文本 `PREPARE` 等 |
| 能否用 GTID 故障转移 / 当 InnoDB 从库？ | **不能** |
| 能否按 MySQL 语义存放不能丢的生产数据？ | **不能** |

### 生产就绪记分卡

| 领域 | 相对 MySQL 8.0 | 能否用于生产 |
|------|----------------|--------------|
| 线握手 + `COM_QUERY` | 官方客户端可用 | 可用于实验 |
| 核心 DML（`INSERT`/`SELECT`/`UPDATE`/`DELETE`） | 已测步骤对齐 | 简单应用可以 |
| 事务 + WAL 重启 | `BEGIN`/`COMMIT`/`ROLLBACK`；快照隔离 | 子集内可持久化；**不是** InnoDB 锁 |
| 模式（`CREATE`/`ALTER`/`INDEX`/`FK`） | 常见模式可用；`TRUNCATE TABLE`（M110）；`REPLACE INTO`（M111）；`INSERT IGNORE`（M112） | 仅开发 / 子集 |
| 查询 SQL（JOIN、`GROUP BY`、子查询、`UNION`、CTE） | 核心形式通过 `mysql-diff` | 只用这些形式可以 |
| 客户端 `SHOW` / `@@` / `information_schema` | 许多目录是**桩** | 连接器能连；运维看板会不准 |
| 权限 | `GRANT`/`REVOKE` + `CREATE USER` MVP | 不是加固安全模型 |
| 复制 | Binlog 行事件 + dump follow MVP | **不是高可用** |
| SQL 函数 | 内置集合在增长 | 缺 `JSON_EXTRACT`/`UUID()`/`GET_LOCK`；`SUBSTRING`/`ROUND`/`DATE_ADD` 可用（M113） |
| 官方 `mysql-test`（数千个 `.test`） | 仅 **100** 条可移植用例 | 不能当作完整度证明 |

**今天适合：** 本地原型、教学、连接器冒烟、给 rusql 贡献代码。  
**今天不适合：** 生产替换 MySQL、任意 mysqldump 恢复、不改 SQL 就跑 WordPress/Rails/Django。

路线图对*客户端可见*表面的定性估计约 **45–70%**（不是版本号）。见 [mysql-full-parity-roadmap.md](../specs/mysql-full-parity-roadmap.md)。

---

## 2. 怎么和 MySQL 对比测试

rusql **没有**宣称通过 Oracle 完整 `mysql-test`。实际语料如下。

| 套件 | 对比什么 | 规模（2026-09-20） | 门禁 |
|------|----------|-------------------|------|
| **`mysql-diff`** | 同一 SQL 在 rusql **和** Docker MySQL 8.0 上跑（官方 `mysql` CLI） | **327 步**，60 套件 + 2 条协议冒烟（318 条不重复文本） | **CI** — 最近一次 **327/327** |
| **`mysql-gap-probe`** | 精选「还缺什么」语句对 rusql（可选 MySQL） | **29 条探测** + 15 条 setup | 仅清单（始终 exit 0） |
| **`mysql-test-subset`** | Oracle mysql-test 的可移植切片，rusql 内部线客户端 | **100 用例**，158 条 SQL | **CI** — 100/100 |
| **`basic.json` 固件** | rusql 线协议 CREATE/INSERT/SELECT/INDEX/WHERE | **18 套件**，101 步 | `cargo test -p rusql-server compat` |
| **`cargo test`** | 单元 + 集成（仅 rusql） | 工作区测试 | **CI** |
| **`sysbench-rusql.mjs`** | 相对 MySQL 的 QPS（`oltp_point_select`） | 少量语句形状、大量迭代 | 手动 / `workflow_dispatch` |
| **`bench-rusql-vs-mysql.mjs`** | 7 个微负载的延迟/QPS | 不是 SQL 覆盖率 | 手动 |

四个 JSON 语料的**不重复 SQL 文本**见 `mysql-diff.json`（已加入 M112 与 M113 套件）。这是语句清单，不是「N 个 MySQL 功能」。

Oracle **mysql-test** 仍有**数千**个 `.test` 文件；几乎全部跳过（[SKIPS.md](../../../tests/mysql-test/SKIPS.md)）。

327 条 `mysql-diff` 中有 **81** 条两边都会执行，但不比对行文本（`compare_output: false`）——通常是允许不同的 `SHOW` / 版本 / 元数据。

### 如何复现

```bash
cargo test -- --skip release_binary
node scripts/mysql-test-subset.mjs
node scripts/mysql-diff.mjs          # 需要 Docker MySQL 8.0
node scripts/mysql-gap-probe.mjs     # 清单；不是通过/失败门禁
```

---

## 3. 进展（相对 MySQL）

| 日期 | 相对 Docker MySQL 8.0 的 `mysql-diff` | 对用户意味着什么 |
|------|--------------------------------------|------------------|
| 2026-06-30 | **7/13** 步对齐（54%） | 官方 CLI：UPDATE/DELETE 报错，跨连接看不到行 |
| 2026-07-03 | 仍是 7/13；协议缺口（#73、#77） | 内部线测试通过；**外部 MySQL 客户端不行** |
| 2026-08-11 | **15/15**；CLI 冒烟 11/11 | 官方客户端在**很小**子集上可用；表面约 15–20% |
| **2026-09-20** | **297/297** 已对比 | 官方客户端在**扩大后的**可移植套件上对齐 MySQL；探测里仍有 26 条 rusql 缺口 |
| **2026-09-20（M110）** | **313/313** 已对比 | 可移植套件加入 `TRUNCATE TABLE`（堆删除全部行 + 重置 `AUTO_INCREMENT`） |
| **2026-09-20（M111）** | **322/322** 已对比 | 可移植套件加入 `REPLACE INTO`（主键先删后插；SELECT 对比 `1,20`） |
| **2026-09-20（M112）** | **327/327** 已对比 | 可移植套件加入 `INSERT IGNORE`（跳过主键冲突；SELECT 对比 `1,10` 与 `2,20`） |
| **2026-09-20（M113）** | 可移植套件 + 函数 | 加入 `SUBSTRING`/`SUBSTR`、`ROUND`、`DATE_ADD` |
| **2026-09-20（Phase Q 退出）** | 缺口探测剩余 **19/29** | M62–M113 完成；官方 MySQL CLI 会话自省无 `unsupported function` |

从 13 步到 297 步，是**更大子集上的更多测试**加上真实的协议/SQL 工作，不是 MySQL 变小了。

---

## 4. 已可用（在已测子集上对齐 MySQL）

**可用**表示：rusql 接受该语句，且 `mysql-diff`（或等价线测试）对可移植用例期望与 MySQL 相近的结果。

### 协议与客户端

| 能力 | rusql | MySQL 8.0 | 证据 |
|------|-------|-----------|------|
| TCP 握手，`SELECT 1` | 可用 | 可用 | `mysql-diff` 协议冒烟 |
| 官方 `mysql` CLI | 可用 | 可用 | `mysql-diff` 两边用同一 CLI |
| `caching_sha2_password` / `mysql_native_password` | 可用（MVP） | 完整插件集 | 线协议 + 认证测试 |
| `COM_QUERY`，`USE` / `COM_INIT_DB` | 可用 | 可用 | CI |
| 二进制预处理（`COM_STMT_*`） | 可用（MVP） | 完整 | rusql 线测试（文本 `PREPARE` **缺失**） |
| `COM_CHANGE_USER` / `COM_RESET_CONNECTION` | 可用 | 可用 | M51 |
| `SHOW PROCESSLIST` | 可用（连接登记） | 完整 processlist | M53；`information_schema.PROCESSLIST` **缺失** |

### DDL 与 DML

| 能力 | rusql | MySQL 8.0 |
|------|-------|-----------|
| `CREATE`/`DROP DATABASE`，`USE` | 可用 | 可用 |
| `CREATE`/`DROP TABLE`，`CREATE INDEX` | 可用 | 可用 |
| `PRIMARY KEY`，`NOT NULL`，`UNIQUE` | 可用 | 可用 |
| `AUTO_INCREMENT` + 无参 `LAST_INSERT_ID()` | 可用 | 可用 |
| `ALTER TABLE` ADD/DROP/MODIFY/RENAME COLUMN，`RENAME TABLE` | 可用 | 更广的 ALTER |
| `FOREIGN KEY` + DML 上 RESTRICT | 可用 | 完整参照动作 |
| `INSERT` / `SELECT` / `UPDATE` / `DELETE` | 可用 | 可用 |
| `TRUNCATE TABLE` | 可用（堆删除全部行 + 重置 AI；不触发 DELETE 触发器） | DDL 截断 / 表空间复用 |
| `REPLACE INTO` | 可用（单列主键先删后插；`affected_rows` 为 1 或 2） | 另有非主键 UNIQUE / 多表 REPLACE |
| `INSERT IGNORE` | 可用（主键冲突跳过；`affected_rows` 为实际插入行数） | 另有非主键 UNIQUE / sql_mode 截断 IGNORE |
| `INSERT … SELECT`，`ON DUPLICATE KEY UPDATE` | 可用（主键 upsert） | 可用 |
| `CREATE TEMPORARY TABLE` | 可用 | 可用 |
| 类型：`INT`、`VARCHAR`、`DECIMAL`、`DATETIME`、`TEXT`、`BLOB`、`JSON`（存储） | 可用 | 完整类型系统 |

### 查询 SQL

| 能力 | rusql | MySQL 8.0 |
|------|-------|-----------|
| 投影、别名、`DISTINCT`、`ORDER BY`、`LIMIT`/`OFFSET` | 可用 | 可用 |
| `WHERE`：比较、`AND`/`OR`/`NOT`、`LIKE`、`BETWEEN`、`IN`、`IS NULL` | 可用 | 可用 |
| `INNER JOIN`，`LEFT`/`RIGHT OUTER JOIN` | 可用 | 可用 |
| `GROUP BY` / `HAVING` + `COUNT`/`SUM`/`MIN`/`MAX`/`AVG` | 可用 | 可用 |
| 子查询：`IN (SELECT)`、`EXISTS`、标量、派生表 | 可用 | 更广 |
| `UNION` / `UNION ALL` | 可用 | 还有 `INTERSECT`/`EXCEPT` |
| 非递归 `WITH` CTE | 可用 | 还有 `WITH RECURSIVE` |
| 窗口：`ROW_NUMBER`/`RANK`/`DENSE_RANK` + `PARTITION BY`/`ORDER BY` | 可用 | 窗口帧与更多函数 |
| `CASE` / `IF(cond, then, else)` | 可用 | 可用 |
| 视图（`CREATE VIEW` + `SELECT`） | 可用 | 缺 `CREATE OR REPLACE VIEW` |

### 会话、表达式、目录（已通过 `mysql-diff`）

| 能力 | 说明 |
|------|------|
| `DATABASE()`/`SCHEMA()`，`USER()`/`CURRENT_USER()`，`VERSION()` | 会话信息 |
| `CONNECTION_ID()`，`ROW_COUNT()`，`FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS` | 已测用例对齐 |
| 算术、`CONCAT`、`COALESCE`/`IFNULL`/`NULLIF`、`CAST`、`NOW`/`CURDATE`、`LENGTH`/`LOWER`/`UPPER`、`SUBSTRING`/`SUBSTR`、`ROUND`、`DATE_ADD` | 内置函数包 |
| `utf8mb4_unicode_ci` / `utf8mb4_0900_ai_ci` 比较/排序（样本语料） | 不是全部校对规则 |
| `EXPLAIN` + 代价规划器索引/范围路径 | 形状，不是 InnoDB EXPLAIN |
| 事务 `BEGIN`/`COMMIT`/`ROLLBACK`；WAL 重启后仍在 | 快照隔离（MVCC） |
| `CREATE PROCEDURE`/`FUNCTION`/`TRIGGER`/`EVENT`（MVP）+ `CALL` | 受限方言；见「部分实现」 |
| 事件调度 `AT` / `EVERY` / `STARTS`/`ENDS` / `DEFINER` / `ON COMPLETION` | 在下一次 `COM_QUERY` 上执行，不是定时线程 |

覆盖以上内容的 `mysql-diff` 套件包括 `portable_dml`、`extended_where`、`outer_join`、`group_by_aggregate`、`subquery_*`、`union_queries`、`with_cte`、`window_functions`、`insert_select`、`on_duplicate_key_update`、`replace_into`、`insert_ignore`、`substring_round_date_add`、`foreign_key_restrict`、`alter_table_extended`、`auto_increment`、`last_insert_id`、`session_info`、`case_if`、事件调度套件等，完整列表见 `crates/rusql-server/compat/mysql-diff.json`。

---

## 5. 部分实现：能执行，但不是 MySQL 等价

这些语句常常**成功**，以便客户端和 ORM 能连上。不要当成 InnoDB。

| 功能 | rusql 做什么 | MySQL 8.0 做什么 |
|------|--------------|------------------|
| `SHOW VARIABLES` / `@@` 桩 | 小型文档化目录；`SET SESSION` 仅内存 | 数百个实时变量 |
| `SET GLOBAL` | 拒绝（errno 1229） | 服务器范围 |
| `SET TRANSACTION ISOLATION LEVEL` | 只覆盖 `@@transaction_isolation` | 改变引擎隔离级别 |
| 引擎隔离 | 快照（MVCC） | InnoDB REPEATABLE READ + next-key 锁 |
| `SELECT … FOR UPDATE` / `FOR SHARE` / `SKIP LOCKED` | **空操作** — 同行，不等待 | 行锁 |
| `SHOW STATUS` / `SHOW TABLE STATUS` / `SHOW ENGINES` | 常量 / 桩单元格 | 实时引擎统计 |
| `SHOW WARNINGS` / `SHOW ERRORS` | 空列表（除非填入） | 实时诊断 |
| `SHOW CREATE DATABASE` | 桩 `utf8mb4` DDL | 每库真实字符集 |
| `SHOW CREATE PROCEDURE`/`FUNCTION` | 重建函数体；**参数列表为空** | 真实参数、DEFINER、sql_mode |
| `SHOW TRIGGERS` 的 Definer / 时间戳 | 桩（`root@%`、空日期） | 实时 |
| `SHOW CREATE USER` | 插件名，**无密码哈希** | 完整 `CREATE USER` 转储 |
| `GRANT`/`REVOKE` | MVP 权限检查 | 完整权限表 |
| 存储过程 / 函数 | `BEGIN…END` MVP；无 `IN`/`OUT`/`SIGNAL` | 完整存储程序 |
| 触发器 | BEFORE INSERT；AFTER UPDATE/DELETE | 全部时机及更多 |
| 事件 | 目录 + COM_QUERY 调度；已持久化 COMMENT；`information_schema.EVENTS` | 定时线程 |
| `information_schema` | 虚拟子集（`TABLES`、`COLUMNS`、`SCHEMATA`、`STATISTICS`、`ROUTINES`、`TRIGGERS`、`EVENTS` 等） | 完整目录 |
| Binlog / 从库 | COMMIT 上行事件；`COM_BINLOG_DUMP` follow；GTID **桩** | 生产复制 + GTID 故障转移 |
| 握手 `VERSION()` | `8.0.33-rusql` | Oracle 版本串 |
| JSON 类型 | 可存储；**缺 `JSON_EXTRACT`** | 完整 JSON 函数 |
| 事件间隔 `MONTH`/`YEAR` | 按 30/365 天近似 | 日历月/年 |

---

## 6. 不可用（相对 MySQL 失败或未实现）

来自 **2026-09-20（M113 之后）缺口探测**：29 条探测，**19 条 rusql 缺口**，9 条成功（含 M108–M113 以及 `CREATE TEMPORARY TABLE`、`UNIQUE`），1 条两边失败（`CREATE PROCEDURE … IN`，因 `DELIMITER` / 方言）。Phase Q 会话自省退出标准已满足（官方 `mysql:8.0` CLI：`DATABASE`/`USER`/`VERSION`/`CONNECTION_ID`/`@@`/`SHOW VARIABLES`/`SET NAMES` 无 `unsupported function`）。`information_schema.EVENTS` 可查询（不是 errno 1146），空目录时批量表头相对 MySQL 可能 `shape_mismatch`。

### 已立案的 Phase Q Issue（已完成）

| SQL / 功能 | rusql | MySQL 8.0 | Issue |
|------------|-------|-----------|-------|
| `CREATE EVENT … COMMENT '…'` | 完成（M108） | 持久化注释 | [M108 #250](https://github.com/tanbamboo/rusql/issues/250) |
| `information_schema.EVENTS` | 完成（M109） | 目录视图 | [M109 #251](https://github.com/tanbamboo/rusql/issues/251) |
| `TRUNCATE TABLE` | 完成（M110） | 堆删除全部行 + 重置 `AUTO_INCREMENT` | [M110 #252](https://github.com/tanbamboo/rusql/issues/252) |
| `REPLACE INTO` | 完成（M111） | 单列主键先删后插 | [M111 #253](https://github.com/tanbamboo/rusql/issues/253) |
| `INSERT IGNORE` | 完成（M112） | 跳过主键冲突，插入其余行 | [M112 #254](https://github.com/tanbamboo/rusql/issues/254) |
| `SUBSTRING` / `ROUND` / `DATE_ADD` | 完成（M113） | 内置函数 | [M113 #255](https://github.com/tanbamboo/rusql/issues/255) |

### Phase Q 之后的探测缺口（Phase R 已立案）

| SQL / 功能 | 典型生产影响 | Issue |
|------------|--------------|-------|
| `CREATE DATABASE … CHARACTER SET … COLLATE …` | 库级字符集 | [M114 #265](https://github.com/tanbamboo/rusql/issues/265) |
| `JSON_EXTRACT(...)` | 应用里的 JSON 列 | [M115 #266](https://github.com/tanbamboo/rusql/issues/266) |
| `UUID()` | 生成标识 | [M116 #267](https://github.com/tanbamboo/rusql/issues/267) |
| `LAST_INSERT_ID(expr)` | 序列辅助 | [M117 #268](https://github.com/tanbamboo/rusql/issues/268) |
| `GET_LOCK(...)` | 应用层劝告锁 | [M118 #269](https://github.com/tanbamboo/rusql/issues/269) |
| `information_schema.TABLE_CONSTRAINTS` | ORM / 迁移工具 introspect | [M119 #270](https://github.com/tanbamboo/rusql/issues/270) |
| `information_schema.PROCESSLIST` | 监控 | [M120 #271](https://github.com/tanbamboo/rusql/issues/271) |
| `information_schema.PARAMETERS` | 存储程序元数据 | [M121 #272](https://github.com/tanbamboo/rusql/issues/272) |
| `SHOW BINARY LOGS` | 复制运维 | [M122 #273](https://github.com/tanbamboo/rusql/issues/273) |
| `SHOW BINLOG EVENTS` | 复制运维 | [M123 #274](https://github.com/tanbamboo/rusql/issues/274) |
| `CREATE OR REPLACE VIEW` | 视图发布 | [M124 #275](https://github.com/tanbamboo/rusql/issues/275) |
| 文本 `PREPARE` / `EXECUTE` | 很多客户端；二进制 `COM_STMT_*` 已有 | [M125 #276](https://github.com/tanbamboo/rusql/issues/276) |
| `SAVEPOINT` | 嵌套回滚 | [M126 #277](https://github.com/tanbamboo/rusql/issues/277) |
| `WITH RECURSIVE` | 层次查询 | [M127 #278](https://github.com/tanbamboo/rusql/issues/278) |
| `INTERSECT` | 集合 SQL | [M128 #279](https://github.com/tanbamboo/rusql/issues/279) |
| 窗口 `ROWS BETWEEN …` 帧 | 分析 SQL | [M129 #280](https://github.com/tanbamboo/rusql/issues/280) |
| `CREATE EVENT … DISABLE ON SLAVE` | 从库事件控制 | [M130 #281](https://github.com/tanbamboo/rusql/issues/281) |
| `SHOW ENGINE INNODB STATUS` | DBA 运维 | [M131 #282](https://github.com/tanbamboo/rusql/issues/282) |
| `CREATE PROCEDURE … IN` | 存储过程参数 | [M132 #283](https://github.com/tanbamboo/rusql/issues/283) |

### 后续阶段已立案（未打 agent-ready）

阶段 S–Z 已有 GitHub Issue 与 `.github/issue-bodies/` 规格（里程碑 10–17）。须等 M114 合入 `main` 且依赖就绪后再开工。当前均**未**打 `agent-ready`。

| 阶段 | 范围 | 里程碑 | Issue |
|------|------|--------|-------|
| S JSON/查询包 | M133–M145 | [10](https://github.com/tanbamboo/rusql/milestone/10) | [#285](https://github.com/tanbamboo/rusql/issues/285)–[#297](https://github.com/tanbamboo/rusql/issues/297) |
| T Schema | M146–M156 | [11](https://github.com/tanbamboo/rusql/milestone/11) | [#298](https://github.com/tanbamboo/rusql/issues/298)–[#308](https://github.com/tanbamboo/rusql/issues/308) |
| U 锁 | M157–M164 | [12](https://github.com/tanbamboo/rusql/milestone/12) | [#309](https://github.com/tanbamboo/rusql/issues/309)–[#316](https://github.com/tanbamboo/rusql/issues/316) |
| V 存储程序 | M165–M172 | [13](https://github.com/tanbamboo/rusql/milestone/13) | [#317](https://github.com/tanbamboo/rusql/issues/317)–[#324](https://github.com/tanbamboo/rusql/issues/324) |
| W 复制 | M173–M180 | [14](https://github.com/tanbamboo/rusql/milestone/14) | [#325](https://github.com/tanbamboo/rusql/issues/325)–[#332](https://github.com/tanbamboo/rusql/issues/332) |
| X 安全/TLS | M181–M188 | [15](https://github.com/tanbamboo/rusql/milestone/15) | [#333](https://github.com/tanbamboo/rusql/issues/333)–[#340](https://github.com/tanbamboo/rusql/issues/340)（`needs-human`） |
| Y 可观测 | M189–M195 | [16](https://github.com/tanbamboo/rusql/milestone/16) | [#341](https://github.com/tanbamboo/rusql/issues/341)–[#345](https://github.com/tanbamboo/rusql/issues/345)、[#347](https://github.com/tanbamboo/rusql/issues/347)、[#348](https://github.com/tanbamboo/rusql/issues/348) |
| Z 剩余表面 | M196–M210 | [17](https://github.com/tanbamboo/rusql/milestone/17) | [#349](https://github.com/tanbamboo/rusql/issues/349)–[#363](https://github.com/tanbamboo/rusql/issues/363) |

**北极星目标尚未完成。** 完成定义是 M209/M210。

### rusql 不宣称覆盖的更大 MySQL 表面

- 完整 InnoDB（表空间、与 Oracle 相同的崩溃恢复、XA、gap lock）
- 分区、生成列、FK 以外的 check 约束
- 完整 `performance_schema`、插件、UDF .so
- GIS、全文、空间索引
- 角色图、TLS、企业认证插件
- Group Replication / InnoDB Cluster / clone
- 与 MySQL 功能对等的代价优化器
- 恢复任意现有生产库的 `mysqldump`

---

## 7. SQL 函数速查

| 函数 / 表达式 | rusql | MySQL 8.0 |
|---------------|-------|-----------|
| `CONCAT`，`LENGTH`，`LOWER`，`UPPER` | 可用 | 可用 |
| `COALESCE`，`IFNULL`，`NULLIF`，`CAST` | 可用 | 可用 |
| `NOW`，`CURDATE` | 可用 | 更完整的日期 API |
| `CASE`，`IF(...)` | 可用 | 可用 |
| `COUNT`/`SUM`/`MIN`/`MAX`/`AVG` | 可用 | 可用 |
| `DATABASE`，`USER`，`VERSION`，`CONNECTION_ID`，`ROW_COUNT` | 可用 | 可用 |
| `LAST_INSERT_ID()` | 可用 | 可用 |
| `LAST_INSERT_ID(expr)` | **缺失** | 可用 |
| `FOUND_ROWS` | 可用 | 8.0.17+ 已弃用但仍存在 |
| `ROW_NUMBER`/`RANK`/`DENSE_RANK` | 可用（无窗口帧） | 窗口帧与更多窗口函数 |
| `SUBSTRING`，`ROUND`，`DATE_ADD` | 可用（M113：1-based 截取；远离零四舍五入；`DATE_ADD` INTERVAL；`MONTH`/`YEAR` 为 30/365 天近似） | 可用 |
| `JSON_EXTRACT`，`UUID`，`GET_LOCK` | **缺失** | 可用 |

---

## 8. 性能（不是兼容分数）

吞吐数字**不能**代替 SQL 覆盖。2026 年 8 月单连接 CLI 循环（[详情](performance-benchmark-2026-08-11.md)）：

| 负载 | 当时 rusql / MySQL QPS |
|------|------------------------|
| `SELECT 1` | 1.55× |
| 主键点查 | 0.92× |
| 索引查找 | 0.91× |
| 扫描 + `ORDER BY` + `LIMIT` | 0.74× |
| 单行 `INSERT` | 1.96× |
| 主键 `UPDATE` | 0.62× |
| `BEGIN`+`INSERT`+`COMMIT` | 0.92× |

这些运行包含 CLI 拉起开销。Sysbench `oltp_point_select` 是行业形态的读门禁（`scripts/sysbench-rusql.mjs`，工具可用时默认 rusql ≥ MySQL 的 0.7×）。两个基准都不扩大 SQL 表面。

---

## 9. 使用决策

| 你的情况 | 用 rusql？ |
|----------|------------|
| 学习 MySQL 协议 / 给 rusql 贡献 | **可以** |
| 新应用只用上文「可用」表，并能接受快照隔离 | **也许**（开发 / 非关键） |
| 已有 MySQL 应用、未知 SQL、dump、带迁移的 ORM | **还不行** |
| 需要 JSON 提取 / 劝告锁 | **还不行**（积压中） |
| 需要 InnoDB 锁、XA、GTID 故障转移、运维 `SHOW ENGINE` | **不行** |
| 生产数据、合规、多 AZ 高可用 | **不行** — 用 MySQL 8.0 或生产级分支 |

拿不准时，把你自己的 SQL 两边都跑一遍：

```bash
# rusql
mysql -h 127.0.0.1 -P 3307 -u root --protocol=TCP

# 用同一批语句对比
node scripts/mysql-diff.mjs
node scripts/mysql-gap-probe.mjs
```

---

## 10. 相关文档

| 文档 | 作用 |
|------|------|
| [用户指南](../user-guide.md) | 如何运行与验证功能 |
| [完整对齐路线图](../specs/mysql-full-parity-roadmap.md) | 里程碑积压（M108+） |
| [版本说明](../release-notes.md) | `main` 已合入内容 |
| [mysql-test SKIPS](../../../tests/mysql-test/SKIPS.md) | 为何 Oracle MTR 不是门禁 |

*`mysql-diff` 步数、缺口探测结果或生产结论变化时，请更新本报告。*

# MySQL 8.0 完全对等路线图（M36+）

**北极星目标**：在 wire 协议、SQL、元数据、安全、复制与可观测行为上与 MySQL 8.0 功能等价——通过不断扩展的 `mysql-test` 语料与业界基准验证。

**基线（2026-08-11）**：M0–M35 已合并；第三方 CLI 冒烟 11/11；`mysql-diff` 15/15；估算覆盖 MySQL 约 15–20%。见 [性能基准报告](../reports/performance-benchmark-2026-08-11.md)。

**当前对比（2026-09-20）**：[rusql 与 MySQL 测试报告](../reports/rusql-vs-mysql.md) — `mysql-diff` 297/297；不能作为生产即插即用替代。

**上一阶段路线图（M0–M35）**：[mysql-compat-roadmap.md](../../en/specs/mysql-compat-roadmap.md)（英文 canonical）

**完整英文版**：[docs/en/specs/mysql-full-parity-roadmap.md](../../en/specs/mysql-full-parity-roadmap.md)

---

## 策略摘要

1. **按类别纵向切片** — 每个差距类别对应一个 GitHub Issue（M36–M61），含可测试验收标准。
2. **兼容反馈闭环** — 每个里程碑合并前扩展 `mysql-diff` 和/或 `mysql-test` 子集。
3. **性能并行推进** — PERF-B* Issue 针对 2026-08-11 基线做 harness 与热点优化。
4. **Agent 循环** — 依赖合并且文件边界清晰后再打 `agent-ready` 标签。

---

## 阶段概览

| 阶段 | 范围 | 里程碑 |
|------|------|--------|
| **H** DDL 与目录 | 多库、AUTO_INCREMENT、扩展 ALTER、外键、类型 | M36–M40 |
| **I** SQL 查询 | 外连接、子查询、GROUP BY、UNION、扩展 WHERE、函数 | M41–M46 |
| **J** 存储程序 | 存储过程/函数、触发器 | M47–M48 |
| **K** 优化器 | 代价模型、复合索引 | M49–M50 |
| **L** 线协议 | CHANGE_USER、FIELD_LIST、PROCESSLIST | M51–M53 |
| **M** 安全 | GRANT/REVOKE、多用户与 native 密码 | M54–M55-auth |
| **N** 复制 | 生产 binlog、Replica、GTID | M56–M58 |
| **O** 字符集 | utf8mb4 完整排序/比较 | M59 |
| **P** 兼容 harness | mysql-test 扩展、Sysbench schema | M60–M61 |

---

## 性能轨道（PERF-B*）

基线：[performance-benchmark-2026-08-11.md](../reports/performance-benchmark-2026-08-11.md)

| ID | 标题 | 优先级 | 基线差距 |
|----|------|--------|----------|
| PERF-B1 | 长连接 benchmark harness | P1 | 消除 CLI 进程开销 |
| PERF-B2 | 扫描 + ORDER BY + LIMIT 优化 | P1 | rusql 0.74× MySQL QPS |
| PERF-B3 | 主键 UPDATE 路径优化 | P1 | rusql 0.62× MySQL QPS |
| PERF-B4 | 多线程 benchmark（1/4/8/16） | P2 | 并发能力未知 |
| PERF-B5 | WAL fsync 策略与吞吐调优 | P2 | 持久化/延迟权衡 |
| PERF-B6 | Sysbench oltp_point_select CI 门禁 | P2 | 业界 OLTP 读标准 |

**拉伸目标**：在 PERF-B1 长连接、10 万行、单线程下，点查/索引读/扫描排序/主键 UPDATE 与 MySQL 8.0 差距 ≤10%。

---

## 覆盖率估算

| 完成阶段 | 约 MySQL 覆盖面 |
|----------|----------------|
| M35（当前） | ~15–20% |
| H + I（M40、M45） | ~35% |
| K + P（M50、M60） | ~45% |
| J + M + N | ~70% |
| 全部 + PERF | 生产可信的对等路径 |

完整 100% 对等（所有引擎/插件/边界）仍是多年工程；本路线图优先 **客户端可见** 的等价性。该估计在实践中的含义见 [rusql 与 MySQL 测试报告](../reports/rusql-vs-mysql.md)。

---

## 阶段 Q — 对等之后的客户端 SQL（M62+）

M36–M61 与 PERF-B* 已落地。后续聚焦高 ROI 的客户端/ORM 缺口。

| ID | 标题 | 优先级 | Issue |
|----|------|--------|-------|
| M62 | `utf8mb4_0900_ai_ci` 排序规则 | P2 | [#153](https://github.com/tanbamboo/rusql/issues/153) |
| M65 | `DATABASE`/`USER`/`VERSION` 会话信息函数 | P1 | [#163](https://github.com/tanbamboo/rusql/issues/163) |
| M66 | `CASE` / `IF` 表达式 | P1 | [#165](https://github.com/tanbamboo/rusql/issues/165) |
| M67 | `SELECT DISTINCT` | P1 | [#167](https://github.com/tanbamboo/rusql/issues/167) |
| M68 | `INSERT … SELECT` / `ON DUPLICATE KEY UPDATE` | P1 | [#169](https://github.com/tanbamboo/rusql/issues/169) |
| M69 | 非递归 `WITH`（CTE） | P1 | [#171](https://github.com/tanbamboo/rusql/issues/171) |
| M70 | 窗口排名（`ROW_NUMBER` / `RANK` / `DENSE_RANK`） | P1 | [#173](https://github.com/tanbamboo/rusql/issues/173) |
| M71 | 按事件拆分 `COM_BINLOG_DUMP` | P1 | [#175](https://github.com/tanbamboo/rusql/issues/175) |
| M72 | INSERT 的 `TABLE_MAP` + `WRITE_ROWS` | P1 | [#177](https://github.com/tanbamboo/rusql/issues/177) |
| M73 | 持续跟随 `COM_BINLOG_DUMP` | P1 | [#179](https://github.com/tanbamboo/rusql/issues/179) |
| M74 | UPDATE/DELETE 行事件 | P1 | [#181](https://github.com/tanbamboo/rusql/issues/181) |
| M75 | `LAST_INSERT_ID()` | P1 | [#183](https://github.com/tanbamboo/rusql/issues/183) |
| M76 | `CONNECTION_ID()` / `ROW_COUNT()` | P1 | [#185](https://github.com/tanbamboo/rusql/issues/185) |
| M77 | `@@` 会话/系统变量 | P1 | [#187](https://github.com/tanbamboo/rusql/issues/187) |
| M78 | `FOUND_ROWS()` / `SQL_CALC_FOUND_ROWS` | P1 | [#189](https://github.com/tanbamboo/rusql/issues/189) |
| M79 | 更多面向连接器握手的 `@@` stub | P1 | [#191](https://github.com/tanbamboo/rusql/issues/191) |
| M80 | `SHOW VARIABLES` stub 目录 | P1 | [#193](https://github.com/tanbamboo/rusql/issues/193) |
| M81 | `SET @@` / 会话变量持久化 | P1 | [#195](https://github.com/tanbamboo/rusql/issues/195) |
| M82 | `SET NAMES` / 用户变量 `@foo` | P1 | [#197](https://github.com/tanbamboo/rusql/issues/197) |
| M83 | `SET CHARACTER SET` / `SELECT @foo := expr` | P1 | [#199](https://github.com/tanbamboo/rusql/issues/199) |
| M84 | `SET TRANSACTION ISOLATION LEVEL` | P1 | [#201](https://github.com/tanbamboo/rusql/issues/201) |
| M85 | `SELECT … FOR UPDATE` 探测/空操作 | P1 | [#203](https://github.com/tanbamboo/rusql/issues/203) |
| M86 | `SHOW STATUS` stub | P1 | [#205](https://github.com/tanbamboo/rusql/issues/205) |
| M87 | `SHOW TABLE STATUS` stub | P1 | [#207](https://github.com/tanbamboo/rusql/issues/207) |
| M88 | `SHOW ENGINES` stub | P1 | [#209](https://github.com/tanbamboo/rusql/issues/209) |
| M89 | `SHOW CHARACTER SET` stub | P1 | [#211](https://github.com/tanbamboo/rusql/issues/211) |
| M90 | `SHOW WARNINGS` stub | P1 | [#213](https://github.com/tanbamboo/rusql/issues/213) |
| M91 | `SHOW CREATE DATABASE` stub | P1 | [#215](https://github.com/tanbamboo/rusql/issues/215) |
| M92 | `SHOW CREATE VIEW` stub | P1 | [#217](https://github.com/tanbamboo/rusql/issues/217) |
| M93 | `SHOW TRIGGERS` stub | P1 | [#219](https://github.com/tanbamboo/rusql/issues/219) |
| M94 | `SHOW CREATE TRIGGER` stub | P1 | [#221](https://github.com/tanbamboo/rusql/issues/221) |
| M95 | `SHOW CREATE PROCEDURE` stub | P1 | [#223](https://github.com/tanbamboo/rusql/issues/223) |
| M96 | `SHOW CREATE FUNCTION` stub | P1 | [#225](https://github.com/tanbamboo/rusql/issues/225) |
| M97 | `SHOW PROCEDURE STATUS` stub | P1 | [#227](https://github.com/tanbamboo/rusql/issues/227) |
| M98 | `SHOW FUNCTION STATUS` stub | P1 | [#229](https://github.com/tanbamboo/rusql/issues/229) |
| M99 | `SHOW CREATE USER` stub | P1 | [#232](https://github.com/tanbamboo/rusql/issues/232) |
| M100 | `SHOW CREATE EVENT` stub | P1 | [#234](https://github.com/tanbamboo/rusql/issues/234) |
| M101 | `SHOW EVENTS` stub | P1 | [#236](https://github.com/tanbamboo/rusql/issues/236) |
| M102 | `CREATE EVENT` 目录 | P1 | [#238](https://github.com/tanbamboo/rusql/issues/238) |
| M103 | `ALTER EVENT` 目录 | P1 | [#240](https://github.com/tanbamboo/rusql/issues/240) |
| M104 | 事件调度器执行到期的 `AT` 事件 | P1 | [#242](https://github.com/tanbamboo/rusql/issues/242) |
| M105 | 事件调度器执行 `EVERY` 周期 | P1 | [#244](https://github.com/tanbamboo/rusql/issues/244) |
| M106 | 事件调度器 `STARTS` / `ENDS` | P1 | [#246](https://github.com/tanbamboo/rusql/issues/246) |
| M107 | 事件 `DEFINER` / `ON COMPLETION` | P1 | [#248](https://github.com/tanbamboo/rusql/issues/248) |
| M108 | 事件 `COMMENT` 持久化 | P1 | [#250](https://github.com/tanbamboo/rusql/issues/250) |
| M109 | `information_schema.EVENTS` | P1 | [#251](https://github.com/tanbamboo/rusql/issues/251) |
| M110 | `TRUNCATE TABLE` | P1 | [#252](https://github.com/tanbamboo/rusql/issues/252) |
| M111 | `REPLACE INTO` | P1 | [#253](https://github.com/tanbamboo/rusql/issues/253) |
| M112 | `INSERT IGNORE` | P1 | [#254](https://github.com/tanbamboo/rusql/issues/254) |
| M113 | `SUBSTRING` / `ROUND` / `DATE_ADD` | P1 | [#255](https://github.com/tanbamboo/rusql/issues/255) |

**状态（2026-09-20）**：已立案表 M62–M113 已全部合入 `main`（最后一项：M113 [PR #263](https://github.com/tanbamboo/rusql/pull/263)）。**退出标准已满足**：官方 MySQL CLI 会话自省（`DATABASE`/`USER`/`VERSION`/`CONNECTION_ID`/`@@`/`SHOW VARIABLES`/`SET NAMES`）不再返回 `unsupported function`。

---

## 阶段 R — Phase Q 之后的高 ROI 客户端 SQL（M114–M132）

缺口探测 29 条、**19 条 rusql 缺口**。GitHub 里程碑：[Phase R — Post-Q client SQL (M114–M132)](https://github.com/tanbamboo/rusql/milestone/9)。首个 `agent-ready`：M114。完整验收标准与文件边界见英文 canonical 与 `.github/issue-bodies/`。

| ID | 标题 | 优先级 | Issue |
|----|------|--------|-------|
| M114 | `CREATE DATABASE … CHARACTER SET` / `COLLATE` | P0 | [#265](https://github.com/tanbamboo/rusql/issues/265) |
| M115 | `JSON_EXTRACT`（`$.key`） | P1 | [#266](https://github.com/tanbamboo/rusql/issues/266) |
| M116 | `UUID()` | P1 | [#267](https://github.com/tanbamboo/rusql/issues/267) |
| M117 | `LAST_INSERT_ID(expr)` 赋值 | P1 | [#268](https://github.com/tanbamboo/rusql/issues/268) |
| M118 | `GET_LOCK` / `RELEASE_LOCK` | P1 | [#269](https://github.com/tanbamboo/rusql/issues/269) |
| M119 | `information_schema.TABLE_CONSTRAINTS` | P1 | [#270](https://github.com/tanbamboo/rusql/issues/270) |
| M120 | `information_schema.PROCESSLIST` | P1 | [#271](https://github.com/tanbamboo/rusql/issues/271) |
| M121 | `information_schema.PARAMETERS` | P2 | [#272](https://github.com/tanbamboo/rusql/issues/272) |
| M122 | `SHOW BINARY LOGS` | P1 | [#273](https://github.com/tanbamboo/rusql/issues/273) |
| M123 | `SHOW BINLOG EVENTS` | P1 | [#274](https://github.com/tanbamboo/rusql/issues/274) |
| M124 | `CREATE OR REPLACE VIEW` | P1 | [#275](https://github.com/tanbamboo/rusql/issues/275) |
| M125 | 文本 `PREPARE` / `EXECUTE` / `DEALLOCATE PREPARE` | P1 | [#276](https://github.com/tanbamboo/rusql/issues/276) |
| M126 | `SAVEPOINT` / `ROLLBACK TO` / `RELEASE` | P1 | [#277](https://github.com/tanbamboo/rusql/issues/277) |
| M127 | `WITH RECURSIVE` | P1 | [#278](https://github.com/tanbamboo/rusql/issues/278) |
| M128 | `INTERSECT` | P2 | [#279](https://github.com/tanbamboo/rusql/issues/279) |
| M129 | 窗口 `ROWS BETWEEN` 帧 | P2 | [#280](https://github.com/tanbamboo/rusql/issues/280) |
| M130 | `CREATE EVENT … DISABLE ON SLAVE` | P2 | [#281](https://github.com/tanbamboo/rusql/issues/281) |
| M131 | `SHOW ENGINE INNODB STATUS` stub | P2 | [#282](https://github.com/tanbamboo/rusql/issues/282) |
| M132 | 存储过程 `IN` 参数 | P2 | [#283](https://github.com/tanbamboo/rusql/issues/283) |

**退出标准**：M113 缺口探测集 rusql 单边失败为 0（或文档化 both-fail）。仍**不是**完整 MySQL 8.0（阶段 S–Z 已立案，见下表）。

## 阶段 S — JSON / 查询包（M133–M145）

GitHub 里程碑：[Phase S](https://github.com/tanbamboo/rusql/milestone/10)。**未**打 `agent-ready`（等 Phase R 依赖合入且不与在研 M114 文件边界重叠）。

| ID | 标题 | 优先级 | Issue |
|----|------|--------|-------|
| M133 | `JSON_UNQUOTE` / `->` / `->>` | P1 | [#285](https://github.com/tanbamboo/rusql/issues/285) |
| M134 | `JSON_OBJECT` / `JSON_ARRAY` / `JSON_SET` | P1 | [#286](https://github.com/tanbamboo/rusql/issues/286) |
| M135 | `LAG` / `LEAD` / `SUM() OVER` | P1 | [#287](https://github.com/tanbamboo/rusql/issues/287) |
| M136 | `EXCEPT` / `EXCEPT ALL` | P2 | [#288](https://github.com/tanbamboo/rusql/issues/288) |
| M137 | `FULL OUTER JOIN` | P2 | [#289](https://github.com/tanbamboo/rusql/issues/289) |
| M138 | `VALUES` 行构造器 | P2 | [#290](https://github.com/tanbamboo/rusql/issues/290) |
| M139 | `CAST` / `CONVERT` 字符集 | P2 | [#291](https://github.com/tanbamboo/rusql/issues/291) |
| M140 | 日期包（`DATE_SUB` / `DATEDIFF` / `DATE_FORMAT`） | P1 | [#292](https://github.com/tanbamboo/rusql/issues/292) |
| M141 | 字符串包（`TRIM` / `REPLACE` / `SUBSTRING_INDEX`） | P1 | [#293](https://github.com/tanbamboo/rusql/issues/293) |
| M142 | `GREATEST` / `LEAST` | P2 | [#294](https://github.com/tanbamboo/rusql/issues/294) |
| M143 | `INSERT … SET` | P2 | [#295](https://github.com/tanbamboo/rusql/issues/295) |
| M144 | 多表 `UPDATE`/`DELETE` | P2 | [#296](https://github.com/tanbamboo/rusql/issues/296) |
| M145 | `EXPLAIN FORMAT=JSON` stub | P3 | [#297](https://github.com/tanbamboo/rusql/issues/297) |

## 阶段 T — Schema 完整性（M146–M156）

里程碑：[Phase T](https://github.com/tanbamboo/rusql/milestone/11)。**未** `agent-ready`。

| ID | 标题 | 优先级 | Issue |
|----|------|--------|-------|
| M146 | 生成列（VIRTUAL `AS (expr)`） | P1 | [#298](https://github.com/tanbamboo/rusql/issues/298) |
| M147 | `CHECK` 约束 | P1 | [#299](https://github.com/tanbamboo/rusql/issues/299) |
| M148 | `ENUM` / `SET` | P2 | [#300](https://github.com/tanbamboo/rusql/issues/300) |
| M149 | `DEFAULT (expr)` | P1 | [#301](https://github.com/tanbamboo/rusql/issues/301) |
| M150 | 不可见列/索引 | P3 | [#302](https://github.com/tanbamboo/rusql/issues/302) |
| M151 | 函数索引 | P3 | [#303](https://github.com/tanbamboo/rusql/issues/303) |
| M152 | 分区表 MVP（RANGE） | P2 | [#304](https://github.com/tanbamboo/rusql/issues/304) |
| M153 | `ALTER TABLE … ADD/DROP INDEX` | P1 | [#305](https://github.com/tanbamboo/rusql/issues/305) |
| M154 | `CREATE TABLE … LIKE` / `AS SELECT` | P1 | [#306](https://github.com/tanbamboo/rusql/issues/306) |
| M155 | `RENAME TABLE` 多对 | P2 | [#307](https://github.com/tanbamboo/rusql/issues/307) |
| M156 | `information_schema.COLUMNS` 扩展列 | P1 | [#308](https://github.com/tanbamboo/rusql/issues/308) |

## 阶段 U — 事务与锁（M157–M164）

里程碑：[Phase U](https://github.com/tanbamboo/rusql/milestone/12)。**未** `agent-ready`。

| ID | 标题 | 优先级 | Issue |
|----|------|--------|-------|
| M157 | `SELECT … FOR UPDATE` 等待 | P0 | [#309](https://github.com/tanbamboo/rusql/issues/309) |
| M158 | `READ COMMITTED` vs 快照 | P1 | [#310](https://github.com/tanbamboo/rusql/issues/310) |
| M159 | `SERIALIZABLE` | P2 | [#311](https://github.com/tanbamboo/rusql/issues/311) |
| M160 | 死锁检测 / errno 1213 | P2 | [#312](https://github.com/tanbamboo/rusql/issues/312) |
| M161 | Gap / next-key 锁 | P3 | [#313](https://github.com/tanbamboo/rusql/issues/313) |
| M162 | XA | P3 | [#314](https://github.com/tanbamboo/rusql/issues/314) |
| M163 | `LOCK TABLES` / `UNLOCK TABLES` | P2 | [#315](https://github.com/tanbamboo/rusql/issues/315) |
| M164 | `GET_LOCK` 超时等待 | P2 | [#316](https://github.com/tanbamboo/rusql/issues/316) |

## 阶段 V — 存储程序（M165–M172）

里程碑：[Phase V](https://github.com/tanbamboo/rusql/milestone/13)。**未** `agent-ready`。

| ID | 标题 | 优先级 | Issue |
|----|------|--------|-------|
| M165 | `OUT` / `INOUT` 过程参数 | P2 | [#317](https://github.com/tanbamboo/rusql/issues/317) |
| M166 | `SIGNAL` / `RESIGNAL` | P2 | [#318](https://github.com/tanbamboo/rusql/issues/318) |
| M167 | `DECLARE` 变量 | P1 | [#319](https://github.com/tanbamboo/rusql/issues/319) |
| M168 | `IF` / `WHILE` / `LOOP` / `LEAVE` | P1 | [#320](https://github.com/tanbamboo/rusql/issues/320) |
| M169 | 游标 | P2 | [#321](https://github.com/tanbamboo/rusql/issues/321) |
| M170 | 条件 `HANDLER` | P3 | [#322](https://github.com/tanbamboo/rusql/issues/322) |
| M171 | 触发器六种时机 | P1 | [#323](https://github.com/tanbamboo/rusql/issues/323) |
| M172 | `DELIMITER` / CLI 限制文档化 | P2 | [#324](https://github.com/tanbamboo/rusql/issues/324) |

## 阶段 W — 生产复制（M173–M180）

里程碑：[Phase W](https://github.com/tanbamboo/rusql/milestone/14)。**未** `agent-ready`。

| ID | 标题 | 优先级 | Issue |
|----|------|--------|-------|
| M173 | GTID 事件类型 33 | P1 | [#325](https://github.com/tanbamboo/rusql/issues/325) |
| M174 | Binlog 心跳 | P2 | [#326](https://github.com/tanbamboo/rusql/issues/326) |
| M175 | 实时 `SHOW MASTER STATUS` | P1 | [#327](https://github.com/tanbamboo/rusql/issues/327) |
| M176 | `@@gtid_executed` | P1 | [#328](https://github.com/tanbamboo/rusql/issues/328) |
| M177 | Replica 提升 / failover | P2 | [#329](https://github.com/tanbamboo/rusql/issues/329) |
| M178 | 半同步 ACK stub 或拒绝 | P3 | [#330](https://github.com/tanbamboo/rusql/issues/330) |
| M179 | `CHANGE MASTER TO` / `START SLAVE` | P2 | [#331](https://github.com/tanbamboo/rusql/issues/331) |
| M180 | 行事件 v2 / 部分镜像 | P3 | [#332](https://github.com/tanbamboo/rusql/issues/332) |

## 阶段 X — 安全与 TLS（M181–M188）

里程碑：[Phase X](https://github.com/tanbamboo/rusql/milestone/15)。**`needs-human`**，直至 ADR 被接受。**未** `agent-ready`。

| ID | 标题 | 优先级 | Issue |
|----|------|--------|-------|
| M181 | TLS `--ssl-cert` / `--ssl-key` | P1 | [#333](https://github.com/tanbamboo/rusql/issues/333) |
| M182 | 角色 | P2 | [#334](https://github.com/tanbamboo/rusql/issues/334) |
| M183 | 密码策略 / 过期 | P3 | [#335](https://github.com/tanbamboo/rusql/issues/335) |
| M184 | `REQUIRE SSL` | P2 | [#336](https://github.com/tanbamboo/rusql/issues/336) |
| M185 | 审计日志 | P3 | [#337](https://github.com/tanbamboo/rusql/issues/337) |
| M186 | `mysql.user` 形状 | P2 | [#338](https://github.com/tanbamboo/rusql/issues/338) |
| M187 | `FLUSH PRIVILEGES` | P2 | [#339](https://github.com/tanbamboo/rusql/issues/339) |
| M188 | 企业插件 — 文档化跳过 | P3 | [#340](https://github.com/tanbamboo/rusql/issues/340) |

## 阶段 Y — 可观测性（M189–M195）

里程碑：[Phase Y](https://github.com/tanbamboo/rusql/milestone/16)。#346 是在研 M114 PR，不是本阶段 Issue。**未** `agent-ready`。

| ID | 标题 | 优先级 | Issue |
|----|------|--------|-------|
| M189 | `performance_schema` stub | P2 | [#341](https://github.com/tanbamboo/rusql/issues/341) |
| M190 | 实时 `SHOW STATUS` | P1 | [#342](https://github.com/tanbamboo/rusql/issues/342) |
| M191 | 慢查询日志 | P2 | [#343](https://github.com/tanbamboo/rusql/issues/343) |
| M192 | 通用查询日志 | P3 | [#344](https://github.com/tanbamboo/rusql/issues/344) |
| M193 | 实时 `SHOW ENGINE INNODB STATUS` | P3 | [#345](https://github.com/tanbamboo/rusql/issues/345) |
| M194 | `information_schema.INNODB_*` stub | P3 | [#347](https://github.com/tanbamboo/rusql/issues/347) |
| M195 | 错误日志 `--log-error` | P2 | [#348](https://github.com/tanbamboo/rusql/issues/348) |

## 阶段 Z — 剩余 MySQL 8.0 表面（M196–M210）

里程碑：[Phase Z](https://github.com/tanbamboo/rusql/milestone/17)。**未** `agent-ready`。北极星目标在 M209/M210 证据齐备前**不得**标记完成。

| ID | 标题 | 优先级 | Issue |
|----|------|--------|-------|
| M196 | GIS / `ST_*` | P3 | [#349](https://github.com/tanbamboo/rusql/issues/349) |
| M197 | FULLTEXT | P3 | [#350](https://github.com/tanbamboo/rusql/issues/350) |
| M198 | InnoDB 表空间 / 崩溃恢复等价 | P2 | [#351](https://github.com/tanbamboo/rusql/issues/351) |
| M199 | Group Replication / InnoDB Cluster | P3 | [#352](https://github.com/tanbamboo/rusql/issues/352) |
| M200 | Clone 插件 | P3 | [#353](https://github.com/tanbamboo/rusql/issues/353) |
| M201 | UDF `.so`（文档化跳过） | P3 | [#354](https://github.com/tanbamboo/rusql/issues/354) |
| M202 | 组件 / 插件加载器 | P3 | [#355](https://github.com/tanbamboo/rusql/issues/355) |
| M203 | 窗口 `RANGE BETWEEN` | P2 | [#356](https://github.com/tanbamboo/rusql/issues/356) |
| M204 | 直方图 / 优化器统计 | P2 | [#357](https://github.com/tanbamboo/rusql/issues/357) |
| M205 | Hash join / BNL 代价 | P2 | [#358](https://github.com/tanbamboo/rusql/issues/358) |
| M206 | 临时表 ENGINE 子句 | P2 | [#359](https://github.com/tanbamboo/rusql/issues/359) |
| M207 | `mysql-test` 100→500 | P1 | [#360](https://github.com/tanbamboo/rusql/issues/360) |
| M208 | 缺口探测升为 CI 底线 | P1 | [#361](https://github.com/tanbamboo/rusql/issues/361) |
| M209 | 需求矩阵（目标完成定义） | P0 | [#362](https://github.com/tanbamboo/rusql/issues/362) |
| M210 | 生产即插即用门禁 | P0 | [#363](https://github.com/tanbamboo/rusql/issues/363) |

验收标准与文件边界见英文 canonical 与 `.github/issue-bodies/`。重建：`node scripts/create-phase-s-z-issues.mjs`。

完整英文版：[docs/en/specs/mysql-full-parity-roadmap.md](../../en/specs/mysql-full-parity-roadmap.md)

---

## Issue 索引

Canonical issues **#100–#131**（2026-08-11 创建）。首个 `agent-ready` 特性 Issue：[#109 M45](https://github.com/tanbamboo/rusql/issues/109)。

> **说明**：早期重复批次产生了 #90–#99，请以 #100–#109 为准并关闭重复项。

完整路线图（英文）：[docs/en/specs/mysql-full-parity-roadmap.md](../../en/specs/mysql-full-parity-roadmap.md)

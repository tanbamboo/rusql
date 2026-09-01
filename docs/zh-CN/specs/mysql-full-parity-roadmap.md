# MySQL 8.0 完全对等路线图（M36+）

**北极星目标**：在 wire 协议、SQL、元数据、安全、复制与可观测行为上与 MySQL 8.0 功能等价——通过不断扩展的 `mysql-test` 语料与业界基准验证。

**基线（2026-08-11）**：M0–M35 已合并；第三方 CLI 冒烟 11/11；`mysql-diff` 15/15；估算覆盖 MySQL 约 15–20%。见 [性能基准报告](../reports/performance-benchmark-2026-08-11.md)。

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

完整 100% 对等（所有引擎/插件/边界）仍是多年工程；本路线图优先 **客户端可见** 的等价性。

---

## Issue 索引

Canonical issues **#100–#131**（2026-08-11 创建）。首个 `agent-ready` 特性 Issue：[#109 M45](https://github.com/tanbamboo/rusql/issues/109)。

> **说明**：早期重复批次产生了 #90–#99，请以 #100–#109 为准并关闭重复项。

完整路线图（英文）：[docs/en/specs/mysql-full-parity-roadmap.md](../../en/specs/mysql-full-parity-roadmap.md)

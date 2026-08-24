# 性能基准 — rusql 与 MySQL 8.0 对比

**日期**: 2026-08-11  
**目的**: 为 rusql 后续性能优化阶段建立可复现基线。  
**范围**: 仅覆盖当前已支持的 SQL 子集；不代表功能已与 MySQL 完全等同。

英文 canonical 版本：[docs/en/reports/performance-benchmark-2026-08-11.md](../../en/reports/performance-benchmark-2026-08-11.md)

---

## 1. 功能是否完全一致？

**否。** rusql 的目标是「对真实客户端可信的 MySQL 8.0 线协议 + SQL 子集」，而非复刻 Oracle MySQL 全量能力。

| 维度 | MySQL 8.0 | rusql（2026-08-11 `main`） |
|------|-----------|---------------------------|
| SQL / 协议覆盖面（估算） | 100% | 约 5–20%，按里程碑持续扩展 |
| 官方 mysql-test | 数千用例 | 仅 20 个线协议子集；绝大多数跳过 |
| mysql-diff 差异门禁 | — | 15/15 通过 |
| 第三方官方 CLI 冒烟（2026-08） | — | 11/11 矩阵 + 8/8 单元测试通过 |

### 已实现（概要）

握手、COM_QUERY、预编译语句、USE/COM_INIT_DB、COM_PING；CREATE/索引/DML/部分 DDL；事务与持久化 WAL、MVCC；SHOW/information_schema；caching_sha2 与 utf8mb4 元数据；视图（M33）；binlog QUERY_EVENT 探针（M34，非生产级复制）。

### 主要差异

- **SQL**：无存储过程/触发器/UDF；类型有限；无 AUTO_INCREMENT；大量语句仍不支持  
- **DDL/目录**：无 `CREATE DATABASE`；ALTER 能力有限  
- **优化器**：无代价模型、join cache、复杂 range 优化  
- **复制**：无完整 binlog/Replica/GTID  pipeline  
- **测试**：复制、字符集、优化器、多连接等 mysql-test 套件均跳过  

---

## 2. 业界性能测试方案调研

| 工具 | 用途 | 对 rusql 的适用性 |
|------|------|-------------------|
| **Sysbench** | MySQL OLTP 微基准事实标准 | 部分适用；多数表结构/SQL 超出当前能力 |
| **mysqlslap** | MySQL 自带压测 | 官方 Docker 镜像中未包含 |
| **TPC-C / TPC-H** | 标准 OLTP/OLAP | 范围过大，SQL 不支持 |
| **mysql-test** | 正确性 | 非性能测试 |

**本次采用**：基于**官方 MySQL 8.0 CLI** 的自定义单连接微基准，仅执行 rusql 已支持的 SQL。待能力扩展后再接入 Sysbench 全套件。

---

## 3. 环境与 workload

- **rusql**：release 二进制，端口 3307，数据目录 `.test-data-bench-20260811`  
- **MySQL**：Docker `mysql:8.0`，端口 3308  
- **客户端**：容器内 `mysql` CLI → `host.docker.internal`  
- **表**：`bench_t`，1 万行，索引列 `k`  
- **并发**：单线程，**每条 SQL 单独启动一次 CLI**（含进程与 TCP 开销）  

Workload：`SELECT 1`、主键点查、索引查、扫描+排序+LIMIT、单条 INSERT、主键 UPDATE、`BEGIN+INSERT+COMMIT`。

---

## 4. 结果（QPS，越高越好）

| Workload | rusql | MySQL | rusql/MySQL |
|----------|-------|-------|-------------|
| SELECT 1 | **67.97** | 43.92 | **1.55×** |
| 主键点查 | 52.53 | **57.01** | 0.92× |
| 索引查 | 42.76 | **47.04** | 0.91× |
| 扫描+ORDER BY+LIMIT | 35.93 | **48.43** | 0.74× |
| INSERT | **58.36** | 29.71 | **1.96×** |
| UPDATE | 34.76 | **55.77** | 0.62× |
| 事务 INSERT | 34.00 | **37.00** | 0.92× |

原始数据：仓库根目录 `.bench-rusql.json`、`.bench-mysql-writes.json`（本地产物，未提交）。

---

## 5. 结论与优化方向

**解读注意**：单线程 CLI 循环使绝对 QPS 偏低，Docker 网络额外增加延迟；10k 行小数据集无法反映磁盘 I/O。

| 领域 | 观察 | 建议优化重点 |
|------|------|--------------|
| 简单协议路径 | rusql 在 SELECT 1、单条 INSERT 更快 | 用长连接驱动复测，区分固定开销与执行开销 |
| 点查/索引读 | 与 MySQL 相差约 10% 内 | 可接受 |
| **扫描+排序** | rusql 慢约 **26%** | 排序、LIMIT 下推、内存分配 |
| **主键 UPDATE** | rusql 慢约 **38%** | WAL 刷盘策略、行更新、MVCC |
| 小事务 | 接近持平 | 扩展为多行/高争用场景 |

**下一阶段基准建议**：长连接客户端、Sysbench `oltp_point_select`、多线程 1/4/8/16、WAL  durability 对比、100 万行以上数据集。

---

## 6. 复现

见英文报告第 6 节；功能回归：`cargo test -p rusql-server mysql_cli`、`node scripts/mysql-diff.mjs`。

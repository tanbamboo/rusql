# ADR：M152 RANGE 表分区（磁盘布局）

**状态**：已接受（M152 / [#304](https://github.com/tanbamboo/rusql/issues/304)）  
**日期**：2026-10-16

## 背景

MySQL 8.0 支持 `CREATE TABLE … PARTITION BY RANGE`。rusql 需要最小的 RANGE 实现：INSERT 路由正确，且在分区键等值条件下 SELECT 可分区裁剪，不包含 HASH/LIST/KEY 或子分区。

sqlparser 的 MySQL 方言无法解析 MySQL 的 `(PARTITION p … VALUES LESS THAN …)` 子句；rusql-sql 在生成 AST 前剥离并校验该子句（与函数索引相同模式）。

## 决策

1. **目录**：`TableMeta.range_partition` 保存 `RangePartitionMeta`——分区键列名及有序的 `VALUES LESS THAN` 上界（最后一分区含 `MAXVALUE`）。

2. **物理布局（堆 / WAL）**：`meta` 中保留一个逻辑表键（含 schema 的 storage key）。行数据存放在 **按分区的堆桶** `{table_key}\x1f{partition_name}`。启用分区时逻辑表键本身不存行。

3. **INSERT**：根据行计算分区键，写入唯一分区桶；无匹配分区时返回 MySQL 类错误（无静默默认分区）。

4. **SELECT / 扫描**：全表扫描按目录顺序拼接各分区桶。当执行器（或 EXPLAIN）可证明分区键等值时，只读匹配桶。

5. **分区表索引（MVP）**：不维护跨分区二级索引；分区键等值用裁剪逻辑。主键与分区键相同时走同一路径。

6. **WAL**：`CreateTable` 记录包含 `range_partition`，重放时重建目录与空分区桶。

## 后果

- HASH/LIST/KEY 与 `ALTER TABLE … ADD/DROP PARTITION` 留待后续。
- 分区表上的 DELETE/UPDATE 扫描所有分区桶（非键谓词优先正确性）。
- EXPLAIN 通过 `extra` 列（`partitions: p0`）及更小的行估计体现裁剪。

## 范围外

- 子分区、分区交换、`TRUNCATE PARTITION`、非 INT 分区键（MVP 仅支持整型 `VALUES LESS THAN` 字面量）。

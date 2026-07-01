# M14 — SELECT 列投影

**合并**：PR #35 · Issue #34

## 问题

ORM 与应用很少每次查询都需要全部列。对 `SELECT id FROM users` 仍返回整行会浪费线缆带宽，且掩盖投影逻辑错误，直到生产流量才暴露。

许多框架在 hydration 时发出窄 `SELECT` 列表；若引擎只支持 `*`，驱动可能过度拉取或在 MySQL 兼容检查中失败。

## 设计空间

| 方案 | 优点 | 缺点 |
|------|------|------|
| 扫描后在执行器投影 | 复用扫描/索引路径；实现简单 | 暂无法下推到存储 |
| 存储层投影 | 宽表省内存 | 每个引擎 API 都要懂 catalog |
| 内部始终 `SELECT *` | 代码最少 | 语义错误；破坏兼容 |

## 决策

- 通过 `resolve_projection` 按 catalog 列序解析 `SELECT` 列表。
- 在堆扫描或索引查找**之后**投影（与 `SELECT *` 同路径）。
- `*` 保持直通（`proj_indices = None`）。
- 输出列名来自标识符或别名。

## 内部流程

```
扫描/索引 → finalize_select_rows → project_rows → 结果列
```

`WHERE` 仍在投影前按**表列名**求值。

## 取舍

仅支持列标识符，尚无表达式与聚合。排序与 `LIMIT`（M16–M17）作用于投影后的行。

## 延伸阅读

- Graefe，《Query evaluation techniques for large databases》(1993)
- MySQL 8.0：[SELECT](https://dev.mysql.com/doc/refman/8.0/en/select.html)

## Harness 启示

> 在 `basic_dml` 中增加一条投影查询，是执行器变更最便宜的回归信号。

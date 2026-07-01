# M14 — SELECT 列投影

**合并**：（待合并）· Issue #34

## 问题

ORM 与应用很少每次查询都要全部列。对 `SELECT id FROM users` 仍返回整行浪费带宽，且投影 bug 易拖到生产才发现。

## 设计选择

- 按目录列顺序解析 `SELECT` 列表
- 在扫描 / 索引查找**之后**切片行（与 `SELECT *` 同路径）
- `*` 保持透传

## 取舍

仅支持列标识符 —— 尚无表达式、`COUNT(*)` 或计算列。

## Harness 启示

> 在 **compat 套件**（`basic_dml`）加一条投影查询 ——  executor 变更最便宜的回归信号。

## 延伸阅读

- [m14-select-projection.md](../../../en/specs/m14-select-projection.md)

# M20 — WHERE 比较与 AND

**Issue #43**

## 问题

真实查询使用 `<`、`>`、`<>` 并用 `AND` 组合条件。M4 仅加速 `col = literal`；无比较运算时非等值过滤会错误地返回全表。

## 决策

- 将 `WHERE` 解析为字面量谓词 + `And` 树。
- 可解析为 `i64` 时按数值比较，否则按字符串比较。
- 单独 `=` 仍走索引快路径。

## Harness 启示

> 独立 `where_comparisons` compat 套件，避免破坏 `basic_dml` 行序假设。

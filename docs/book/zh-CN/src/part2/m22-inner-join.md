# M22 — INNER JOIN

**Issue #45**

## 问题

关系型数据分布在多表。应用需要通过 `INNER JOIN ... ON` 组合行（如 `orders` + `order_items`）。

## 决策

- 两表嵌套循环连接；仅支持单个 `INNER JOIN ... ON col = col`。
- 列名为左表列 + 右表列；其后复用 M14–M21 的 WHERE/投影/排序/分页管线。

## Harness 启示

> `inner_join` compat 套件用一对多键验证连接与 WHERE 组合。

# M16 — SELECT LIMIT

**合并**：PR #39 · Issue #38

## 问题

ORM、REST API 与管理界面用 `LIMIT` 分页。没有它则每次查询返回全表扫描结果，对大表不可用，也不符合常见 MySQL 客户端习惯（如 `LIMIT 1` 做存在性检查）。

## 设计空间

| 方案 | 优点 | 缺点 |
|------|------|------|
| 物化后 `take(n)` | 与 WHERE/投影组合简单正确 | 扫描无法提前终止 |
| 下推 `LIMIT` 到索引/扫描 | 更快 | 需规划器 |
| 仅在协议层截断 | — | 语义错误 |

## 决策

- 从 sqlparser 外层 `Query.limit` 读取。
- 在扫描、投影、过滤及（后续）`ORDER BY` **之后** `take(n)`。
- MVP 仅支持整数字面量。

## 内部流程

```
… → 过滤 → 投影 → ORDER BY (M17) → apply_limit → 行集
```

`information_schema` 扫描共用 `finish_rows_query` 路径。

## 取舍

尚无 `OFFSET` 与优化器下推；小堆表可接受。与 `ORDER BY` 的组合语义在 M17 定义。

## 延伸阅读

- MySQL 8.0：[LIMIT](https://dev.mysql.com/doc/refman/8.0/en/select.html)

## Harness 启示

> 在 `basic_dml` 插入数据旁增加 `LIMIT 1` 用例，断言列顺序不变。

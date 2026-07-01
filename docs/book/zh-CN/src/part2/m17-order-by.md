# M17 — SELECT ORDER BY

**Issue #40**

## 问题

客户端假定结果集可按用户指定列排序。堆扫描与索引查找返回插入顺序，会破坏 ORM、报表及兼容测试。MySQL 在投影与过滤之后排序；rusql 对基础表 `SELECT` 需保持一致。

## 设计空间

| 方案 | 优点 | 缺点 |
|------|------|------|
| 执行器扫描后排序 | 简单，与 LIMIT/投影管线一致 | 无法利用索引顺序 |
| 下推到存储/索引 | 索引列更快 | 需规划器与排序元数据 |
| 外排 | 大表可扩展 | MVP 堆表过重 |

## 决策

- 从 sqlparser 读取 `Query.order_by`。
- 按**输出列名**解析排序键（投影之后）。
- 单元格字符串字典序比较；默认 `ASC`，`DESC` 取反。
- 在过滤/投影**之后**、`LIMIT` **之前**应用。

## 内部流程

```
扫描 → 投影 → WHERE → ORDER BY 排序 → LIMIT → 结果集
```

暂不支持 `NULLS FIRST/LAST`、表达式排序键等。

## 延伸阅读

- MySQL 8.0：[ORDER BY 优化](https://dev.mysql.com/doc/refman/8.0/en/order-by-optimization.html)
- Graefe，《Query evaluation techniques for large databases》(1993)

## Harness 启示

> 在 `basic.json` 中与 `LIMIT` 用例并列增加 `ORDER BY`，用线缆兼容测试捕获排序回归。

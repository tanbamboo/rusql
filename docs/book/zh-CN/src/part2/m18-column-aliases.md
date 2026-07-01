# M18 — SELECT 列别名

**Issue #41**

## 问题

API 与 ORM 按列名映射结果。`SELECT id FROM users` 强迫客户端知道物理列名；生产查询用 `AS` 稳定 DTO 字段（`user_id`、`display_name`）。线缆协议列元数据必须反映别名，否则驱动映射错误。

## 设计空间

| 方案 | 优点 | 缺点 |
|------|------|------|
| 仅在 `resolve_projection` 处理别名 | 复用 M14 管线 | `ORDER BY` 须解析别名 |
| 投影后单独别名表 | 职责清晰 | 重复解析 |
| 忽略别名 | — | 破坏兼容 |

## 决策

- M14 已解析 `SelectItem::ExprWithAlias`；M18 **固化测试与文档**。
- 有别名时输出列名为别名，否则为基列名。
- `ORDER BY` 仍按**输出列名**解析（M17）。

## 内部机制

```rust
names.push(alias.unwrap_or(col));
```

投影索引仍指向表列；仅元数据变化。

## 取舍

非标识符表达式的 `AS` 尚未支持。无 `AS` 的隐式别名取决于 sqlparser 方言，非 M18 验收范围。

## 延伸阅读

- MySQL 8.0：[SELECT 别名](https://dev.mysql.com/doc/refman/8.0/en/select.html)

## Harness 启示

> `basic_dml` 增加一条别名用例，断言协议列名而不只是单元格值。

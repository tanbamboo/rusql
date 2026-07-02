# M25 — 二进制结果集（COM_STMT_EXECUTE）

**Issue #48**

## 问题

使用预编译语句的驱动期望**二进制协议**结果行：列元数据带 MySQL `enum_field_types`，`COM_STMT_EXECUTE` 返回带 NULL 位图与按类型编码的值（而非 lenenc 文本）。缺少该能力时 JDBC/ORM 可能无法正确解析整数或拒绝线缆格式。

## 设计

- 将 catalog SQL 类型映射为 MySQL 线缆类型（`INT` → `MYSQL_TYPE_LONG`，`VARCHAR` → `MYSQL_TYPE_VAR_STRING`）。
- `COM_STMT_PREPARE` 的列/参数定义包含映射后的类型字节。
- `COM_STMT_EXECUTE` 的 SELECT 响应使用 `binary_resultset`（行头 0x00，NULL 位图偏移 0）。
- `COM_QUERY` 仍为文本结果集（不变）。

## 实现要点

- 新增 `rusql-protocol::binary`：二进制值与行的编解码。
- `PreparedStatement` 在 prepare 时推断并保存 `result_column_types`。
- 测试客户端解码二进制行以做线缆集成测试。

## Harness 经验

> `stmt_prepare_execute_binary_table_select` 在 TCP 上锁定 INT + VARCHAR 二进制往返。

## 参考

- [MySQL Binary Protocol Resultset](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_binary_resultset.html)

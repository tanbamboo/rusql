# M11 — 预编译语句

**合并**：PR #27 · Issue #26 · [spec](../../../en/specs/m11-stmt-prepare.md)

## 问题

驱动使用带 `?` 的 **`COM_STMT_PREPARE` / `EXECUTE` / `CLOSE`** —— 仅有 `COM_QUERY` 不够。

## 设计选择

- `rusql-protocol` 二进制 stmt OK 包
- 每连接预编译语句存储
- `?` 绑定 → 解析前替换为字面量（MVP 简化）

## 取舍

无二进制结果集、无 `COM_STMT_FETCH`、无 long-data —— spec 中写明边界。

## Harness 启示

> 协议里程碑要**包级单元测试** + 线缆测试 —— 评审中发现 stmt id 从 0 起步的 bug。

## 延伸阅读

- [m11-stmt-prepare.md](../../../en/specs/m11-stmt-prepare.md)

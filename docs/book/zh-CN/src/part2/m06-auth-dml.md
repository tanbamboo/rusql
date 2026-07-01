# M6 — 认证与 DML

**合并**：PR #17 · Issue #16

## 问题

开发模式开放服务器适合折腾；严肃演示需要 **`--auth-password`**，以及 `DROP TABLE`、`DELETE` 完善 DML。

## 设计选择

| 主题 | 选择 |
|------|------|
| 认证 | 可选校验；`mysql_native_password` 扰乱（[adr-m6](../../../en/specs/adr-m6-auth-and-dml.md)） |
| DROP | 目录、引擎、WAL 一致 |
| DELETE | `WHERE col = 字面量` 或全表扫描删行 |

Issue #16 含显式**决策表** —— 人不阻塞 Agent 的异步参与模型。

## 取舍

认证在 M7 前仍为**快路径**；无账号管理、无 `GRANT`。

## Harness 启示

> 产品选择重要时在 **Issue 写决策表**；Agent 实现选中行。

## 延伸阅读

- [adr-m6-auth-and-dml.md](../../../en/specs/adr-m6-auth-and-dml.md)

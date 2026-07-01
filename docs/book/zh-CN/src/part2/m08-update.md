# M8 — UPDATE

**合并**：PR #20

## 问题

没有 **UPDATE** 的 CRUD 无法支撑真实应用与 ORM 冒烟。

## 设计选择

- `UPDATE tbl SET col = 字面量 [WHERE col = 字面量]`
- 复用删除过滤逻辑选行
- WAL 记录持久化更新

## 取舍

仅字面量赋值 —— 无表达式；多列 SET 受解析器能力限制。

## Harness 启示

> M5 之后**每个 DML 里程碑补 compat 步骤** —— UPDATE 仅为小 JSON 差异 + CI 变绿。

# M29 — mysql-diff 运行器

**Issue #52**

## 问题

`basic.json` 兼容测试只覆盖 rusql 协议行为。[Harness 回顾报告](../../../en/reports/harness-retrospective-2026-06-30.md) 要求对真实 MySQL 8.0 做**差异反馈**。

## 决策

- `scripts/mysql-diff.mjs` 构建 `rusql-server`、启动 Docker `mysql:8.0`，并对 `compat/mysql-diff.json` 中每一步对比 `mysql -B` 批输出。
- 仅使用可移植 fixture 子集（不含 `USE rusql`、`information_schema` 或 rusql 专有 DDL）。
- 无 Docker 或 `mysql` 客户端时以 0 退出跳过；CI 任务 `mysql-diff` 在 ubuntu-latest 安装客户端并运行脚本。
- 已记录差距：完整 `basic.json` 无法与 MySQL 逐行对齐；用 `mysql-diff.json` 获取差异信号。

## Harness 经验

> 每个 suite 使用新的 rusql 数据目录与独立 MySQL 库，避免顺序 DDL/DML 在 diff 时串表。

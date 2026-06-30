# ADR：SQL 解析器策略

**状态**：已接受（[#4](https://github.com/tanbamboo/rusql/issues/4#issuecomment-4840905461)）  
**日期**：2026-06-30

## 决策

使用 **`sqlparser`** + **MySQL 方言**（选项 A）；在 `mysql-test-runner` 暴露缺口时做针对性扩展。

英文 canonical：[docs/en/specs/adr-sql-parser.md](../en/specs/adr-sql-parser.md)

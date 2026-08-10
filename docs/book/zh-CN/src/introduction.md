# 引言

本书记录 **rusql** —— Rust 实现的 MySQL 8.0 兼容数据库 —— 在 **Harness Engineering** 与 AI Agent 下如何按里程碑建成。面向**专业软件工程师**：未必做过存储引擎，但读完应能建立协议、目录、执行、持久化与增量交付的清晰心智模型。

## 本书是什么

与 `main` 上真实合并挂钩的**设计叙事**。每章包含：问题、设计空间、决策与取舍、适度内部机制、Harness 启示。

## 不是什么

非 Rust 教程、非 MySQL 手册全文、非源码罗列。操作验证见[用户指南](../../zh-CN/user-guide.md)。

## 阅读顺序

1. [MySQL 兼容全景](part0/mysql-landscape.md)
2. 第一篇 Harness Engineering
3. 第二篇 里程碑 M0–M35
4. [参考书目](appendix/bibliography.md)

## 深度标准（2026 修订）

根据 [#28](https://github.com/tanbamboo/rusql/issues/28) 读者反馈，章节已扩充：更丰富的问题陈述、被拒绝的备选方案、经典文献引用、与生产 MySQL 的差距说明。

## 活文档

路线图见 [mysql-compat-roadmap.md](../../../en/specs/mysql-compat-roadmap.md)（M35 之后见路线图）。

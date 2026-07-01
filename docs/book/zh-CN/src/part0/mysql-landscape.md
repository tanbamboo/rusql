# MySQL 兼容全景

在进入各里程碑之前，先对齐 **「MySQL 8.0 兼容」** 在工程上指什么 —— 以及 rusql 初期**刻意不做**什么。

## 客户端关心的三个面

| 层面 | 出错后果 | rusql 策略（M0–M16） |
|------|----------|----------------------|
| **线缆协议** | 驱动无法连接 | 协议 v10、OK/ERR、文本结果集、`COM_STMT_*` |
| **SQL 子集** | ORM 发出未支持语法 | 递增式执行器；`sqlparser` MySQL 方言 |
| **元数据** | 工具自省失败 | `information_schema`、`DESCRIBE`、`SHOW` |

完整 MySQL 还包括复制、权限图、优化器等。rusql 用**垂直切片**，每次合并增加可测能力，而不假装其余已存在。

## 分层架构（概念）

```
客户端 → 线缆协议 → 会话/目录 → 解析/规划 → 执行器 → 存储引擎
```

与经典教材（见[参考书目](../appendix/bibliography.md)）一致：存储负责持久化与索引；执行层实现关系代数；前端对接客户端语言。

## 为何不 fork MySQL？

目标是适合 Harness Engineering 的**可审计 Rust 代码库**，而非 C++ 血统移植。

## 学术背景

- Codd (1970) — 关系模型
- Mohan et al. (1992) — ARIES 恢复（M3/M31 WAL 方向）
- Comer (1979) — B+ 树（M4 二级索引）
- Berenson et al. (1995) — 隔离级别（M9 覆盖层 vs 未来 M32 MVCC）

## 路线图

详见 [mysql-compat-roadmap.md](../../../en/specs/mysql-compat-roadmap.md)（M17–M35）。

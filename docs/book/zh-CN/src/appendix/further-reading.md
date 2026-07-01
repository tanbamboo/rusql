# 延伸阅读

## 本仓库

- [用户指南](../../../zh-CN/user-guide.md) — 在 `main` 上验证功能
- [版本说明](../../../zh-CN/release-notes.md) — 各里程碑摘要
- [CHANGELOG](../../../../CHANGELOG.md)
- [Harness 回顾报告](../../../en/reports/harness-retrospective-2026-06-30.md)
- [复制 ADR（未来）](../../../en/specs/adr-replication.md)

## 外部

- [Harness Engineering — Martin Fowler](https://martinfowler.com/articles/harness-engineering.html)
- [MySQL 8.0 参考手册](https://dev.mysql.com/doc/refman/8.0/en/)
- [mdBook 文档](https://rust-lang.github.io/mdBook/)
- [sqlparser-rs](https://github.com/sqlparser-rs/sqlparser-rs)

## 贡献新章

M14+ 合并时：

1. 在中英 `SUMMARY.md` 各加一行
2. 撰写中英章节（问题 → 选择 → 取舍 → harness 启示）
3. 运行 `node scripts/check-book.mjs`
4. 若里程碑对用户可见，在 release-notes 中链接

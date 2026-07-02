# M30 — mysql-test 子集

**Issue #53**

## 问题

Oracle **mysql-test** 是 MySQL 官方回归语料（数千个 `.test` 文件）。rusql 需要可运行的小子集，覆盖 mysql-test 主题，而不移植完整的 `mysql-test-run.pl` 框架。

## 决策

- `tests/mysql-test/manifest.json` — 12 个 wire 套件，带 `origin` 字段指向 mysql-test 结果文件（简化 SQL）。
- 共享运行器 `wire_fixtures.rs`（`compat_suite.rs` 亦使用）。
- `cargo test -p rusql-server mysql_test_subset` 或 `node scripts/mysql-test-subset.mjs`。
- 跳过项见 `tests/mysql-test/SKIPS.md`（存储过程、复制、字符集、完整优化器、官方 CLI 差异 — 见 issue #73）。
- CI 任务 `mysql-test-subset` 在 ubuntu-latest 运行脚本（无需 Docker）。

## Harness 经验

> mysql-test 风格用例复用内部 wire 测试客户端；与官方 `mysql` CLI 的差异对比走独立轨道（M29 + #73）。

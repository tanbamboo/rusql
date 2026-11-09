# rusql

使用 Rust 编写的 MySQL 8.0 兼容数据库，采用 [Harness Engineering](https://martinfowler.com/articles/harness-engineering.html) 进行 AI 原生开发。

**English**: [README.md](../../README.md)

## 状态

面向 MySQL 8.0 兼容的 **开发者预览**。**不能作为 MySQL 8.0 的生产即插即用替代。** Phase Z（M199–M210）未关闭前可发布预览 tag；仅在 `main` 具备 M209 + M210 证据后才可宣称公开 GA 即插即用。路径：[公开发布路线图](specs/public-release-roadmap.md) · [English](../en/specs/public-release-roadmap.md)。用户向对比（可用 / 不可用、相对 MySQL 的测试规模）：[rusql 与 MySQL 测试报告](reports/rusql-vs-mysql.md)。

**书籍**（设计叙事 + Harness Engineering）：[docs/book/README.md](../book/README.md)

## 架构

见 [docs/en/architecture/overview.md](../en/architecture/overview.md)。

## 快速开始

```bash
cargo build
cargo test
cargo run -p rusql-server -- --port 3306
```

## 开发

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
```

详见 [AGENTS.md](AGENTS.md) 与 [spec-to-ship.md](../en/workflows/spec-to-ship.md)。

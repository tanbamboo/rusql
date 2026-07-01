# rusql

使用 Rust 编写的 MySQL 8.0 兼容数据库，采用 [Harness Engineering](https://martinfowler.com/articles/harness-engineering.html) 进行 AI 原生开发。

**English**: [README.md](../../README.md)

## 状态

早期开发阶段，持续向 MySQL 8.0 兼容推进。

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

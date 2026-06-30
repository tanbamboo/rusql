# AGENTS.md — 项目契约

> AI 编程 Agent 的入口契约。英文 canonical 版本见 [AGENTS.md](../../AGENTS.md)。

## 项目目的

**rusql** 是使用 Rust 实现的 MySQL 8.0 兼容数据库，采用 Harness Engineering（Agent = Model + Harness）开发。长期目标：完整 MySQL 8.0 兼容；首期从线协议与基础 SQL 起步。

## 仓库地图

完整地图：[.agents/context-index.md](../../.agents/context-index.md)

| 路径 | 用途 |
|------|------|
| `crates/` | Rust workspace 各 crate |
| `profiles/rust/` | Rust 栈 guides 与 sensors |
| `.agents/` | 可移植 agent 层 |
| `docs/en/` | 英文文档（canonical） |
| `docs/zh-CN/` | 简体中文镜像 |

## 常用命令

| 命令 | 说明 |
|------|------|
| `cargo fmt --all` | 格式化 |
| `cargo clippy --all-targets --all-features -- -D warnings` | Lint |
| `cargo test` | 测试 |
| `node scripts/harness-validate.mjs` | 验证 harness 结构 |

## 会话协议

1. **开始**：读取 HANDOFF.md；轮询 `agent-ready` GitHub Issues
2. **规划**：复杂任务使用 Plan 模式
3. **实现**：完成前运行 sensors
4. **结束**：更新 HANDOFF.md

## 核心原则

1. Spec-first（规格优先）
2. 本地门禁 = CI 门禁
3. Hashimoto 闭环
4. 国际化：默认 en-US，同时支持 zh-CN

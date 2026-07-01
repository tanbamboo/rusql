# rusql 的 Harness 技术栈

本章对应每个会话中 Agent 与人实际接触的工件。

## 工作队列

- 带 `agent-ready`、`priority:P0` 的 **GitHub Issue**
- Issue 正文：目标、验收标准、**文件边界**
- `node scripts/check-issue-replies.mjs` 处理 `needs-human`

## 规格

| 工件 | 作用 |
|------|------|
| ADR（`docs/en/specs/adr-*.md`） | 不可逆分叉（认证、解析器） |
| 里程碑 spec（`m9-transactions.md` 等） | 单次 PR 可测切片 |
| Issue 模板（`.github/issue-bodies/`） | 可复用范围合同 |

## 传感器（`profiles/rust/sensors.yaml`）

快速：rustfmt、clippy。标准：`cargo test`。CI 增加 harness-validate、doc-parity、changelog-check、handoff-check。

## 跨会话记忆

- **HANDOFF.md** — 分支、下一步、近期合并
- **CHANGELOG.md** + **release-notes** — 每 PR 用户可见历史（#23 策略）

## 可执行用户合同

- **user-guide**（中英）— 如何在 `main` 上验证
- **compat/basic.json** — 线缆级 SQL 场景（M5+）

## Agent 规则

`.cursor/rules/issue-loop.mdc`：轮询 Issue、功能与文档同船、禁止反复问「是否继续」。

## 设计选择

选用**仓库内 Markdown** 而非 Wiki，以便传感器校验结构、PR 与代码同版。

## 取舍

维护更多文档 —— 用每次合并清单与「仅里程碑级更新」缓解。

## Harness 启示

> 若 Agent **无法在 2 分钟内本地验证**某主张，应先加传感器或夹具，再加功能。

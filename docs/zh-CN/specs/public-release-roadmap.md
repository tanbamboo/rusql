# 公开发布路线图

与宪法对齐的路径：从当前 `main` 到可宣称 **MySQL 8.0 社区版即插即用** 的公开 GA。本文不重新定义北极星目标；它按 [mysql-full-parity-roadmap.md](mysql-full-parity-roadmap.md) 已有的 Phase Z 出口编排预览与打包。

**English**: [public-release-roadmap.md](../../en/specs/public-release-roadmap.md)

**快照日期**：2026-11-10

## 发布类别

| 类别 | 何时允许 | 对外表述 |
|------|----------|----------|
| **开发者预览** | `main` CI 绿灯 | 实验性二进制；**不是** 生产级 MySQL 替代 |
| **公开 GA（可宣称即插即用）** | `main` 上具备 M209 + M210 证据 | 可宣称与 MySQL 8.0 社区功能等价，并链接证据 |
| **企业插件 / UDF `.so` 对等** | 永不要求 | 显式跳过 [M188](https://github.com/tanbamboo/rusql/issues/340)、[M201](https://github.com/tanbamboo/rusql/issues/354) |

公开 GA 指第一个可以宣称 MySQL 8.0 社区版即插即用的 GitHub Release。门槛是 **M210**。更早的 tag 仅属开发者预览（`v0.1.0-preview` 为 Stage 0）。

## 约束规则

各阶段遵循项目契约（[CONSTITUTION.md](../../../CONSTITUTION.md)、[AGENTS.md](../../../AGENTS.md)）：

- Spec-first；不超出 issue 文件边界
- 合并前跑 sensors（`cargo fmt` / `clippy -D warnings` / `test` / `harness-validate`；CI 要求的 mysql-diff / protocol）
- 用户可见字符串走 `rusql-i18n`（en-US + zh-CN）
- 双语文档；一 issue 一 PR；只给下一个序号 issue 打 `agent-ready`
- 不改 `CONSTITUTION.md` / `.github/CODEOWNERS`；不 force-push；不用 `--no-verify`
- 在 M209、M210 证据落地前，HANDOFF 不得写「终极目标已达成」

## 当前状态（证据）

| 层级 | 状态 |
|------|------|
| CI / tip | `main` 绿灯至 M201（[PR #461](https://github.com/tanbamboo/rusql/pull/461)）；含 M199–M201 |
| Phase Q–Y | 完成 |
| Phase Z 已完成 | M196–M201（GIS、FULLTEXT、崩溃恢复、GR 成员 stub、本地 CLONE、UDF `.so` 跳过） |
| Stage 0 | 完成：[v0.1.0-preview](https://github.com/tanbamboo/rusql/releases/tag/v0.1.0-preview)；README/user-guide 标明 **开发者预览** / **不是即插即用** |
| Phase Z 未结 | M202–M210（[#355](https://github.com/tanbamboo/rusql/issues/355)–[#363](https://github.com/tanbamboo/rusql/issues/363)）— **9 个 issue** |
| 下一个 `agent-ready` | [#355 M202](https://github.com/tanbamboo/rusql/issues/355)（另有 `needs-human`，需 ADR） |
| workspace 版本 | GA 切割前保持 `0.1.0` |
| 今日结论 | 可用于原型与贡献 — **不是** 公开 MySQL 替代 |

## 距公开 GA 的缺口

### A. Stage 1 剩余 — 产品面（M202–M206）

| ID | Issue | 优先级 | 缺口 |
|----|-------|--------|------|
| M202 | [#355](https://github.com/tanbamboo/rusql/issues/355) | P3 | 组件 / 插件加载器 — ADR 后文档化拒绝 `INSTALL PLUGIN` 或 rusql 原生子集 |
| M203 | [#356](https://github.com/tanbamboo/rusql/issues/356) | P2 | 命名 `WINDOW` + `RANGE BETWEEN` |
| M204 | [#357](https://github.com/tanbamboo/rusql/issues/357) | P2 | Histogram / 优化器统计持久化 |
| M205 | [#358](https://github.com/tanbamboo/rusql/issues/358) | P2 | Hash join / BNL 代价 |
| M206 | [#359](https://github.com/tanbamboo/rusql/issues/359) | P2 | 临时表 `MEMORY` / InnoDB engine 子句 |

### B. Stage 2 — 证据地板（M207–M208）

| ID | Issue | 优先级 | 缺口 |
|----|-------|--------|------|
| M207 | [#360](https://github.com/tanbamboo/rusql/issues/360) | P1 | 可移植 mysql-test 100 → 500 |
| M208 | [#361](https://github.com/tanbamboo/rusql/issues/361) | P1 | gap probe 成为 CI 地板 |

### C. Stage 3 — 完成定义（M209–M210）

| ID | Issue | 优先级 | 缺口 |
|----|-------|--------|------|
| M209 | [#362](https://github.com/tanbamboo/rusql/issues/362) | P0 | `rusql-vs-mysql.md` 逐项矩阵；范围内无 **Missing** |
| M210 | [#363](https://github.com/tanbamboo/rusql/issues/363) | P0 | dump/restore + ORM + 复制 + 锁相对 Docker `mysql:8.0` 全绿 |

### D. 横切

- M209 前刷新兼容报告；MVP 残留须实现或 ADR Out of scope（禁止静默跳过）
- 仅在 M210 后切 GA 版本；crates.io 需人类批准

## 剩余阶段

- **Stage 0**：已完成（预览 tag + 诚实表述）
- **Stage 1**：完成 M202–M206
- **Stage 2**：M207–M208 harness 地板
- **Stage 3**：M209–M210 GA 门禁
- **Stage 4**：GA 产物发布

## 当前下一步产品 issue

[#355 M202](https://github.com/tanbamboo/rusql/issues/355) 组件 / 插件加载器：接受双语 ADR，然后文档化拒绝或子集。

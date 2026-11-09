# 公开发布路线图

与宪法对齐的路径：从当前 `main` 到可宣称 **MySQL 8.0 社区版即插即用** 的公开 GA。本文不重新定义北极星目标；它按 [mysql-full-parity-roadmap.md](mysql-full-parity-roadmap.md) 已有的 Phase Z 出口编排预览与打包。

**English**: [public-release-roadmap.md](../../en/specs/public-release-roadmap.md)

## 发布类别

| 类别 | 何时允许 | 对外表述 |
|------|----------|----------|
| **开发者预览** | `main` CI 绿灯 | 实验性二进制；**不是** 生产级 MySQL 替代 |
| **公开 GA（可宣称即插即用）** | `main` 上具备 M209 + M210 证据 | 可宣称与 MySQL 8.0 社区功能等价，并链接证据 |
| **企业插件对等** | 永不要求 | 显式跳过 [M188](https://github.com/tanbamboo/rusql/issues/340) |

公开 GA 指第一个可以宣称 MySQL 8.0 社区版即插即用的 GitHub Release。门槛是 **M210**。更早的 tag 仅属开发者预览。

## 约束规则

各阶段遵循项目契约（[CONSTITUTION.md](../../../CONSTITUTION.md)、[AGENTS.md](../../../AGENTS.md)）：

- Spec-first；不超出 issue 文件边界
- 合并前跑 sensors（`cargo fmt` / `clippy -D warnings` / `test` / `harness-validate`；CI 要求的 mysql-diff / protocol）
- 用户可见字符串走 `rusql-i18n`（en-US + zh-CN）
- 双语文档；一 issue 一 PR；只给下一个序号 issue 打 `agent-ready`
- 不改 `CONSTITUTION.md` / `.github/CODEOWNERS`；不 force-push；不用 `--no-verify`
- 在 M209、M210 证据落地前，HANDOFF 不得写「终极目标已达成」

## 当前状态

- Phase Q–Y 已在 `main`；Phase Z 的 M196–M198 已完成（GIS 子集、FULLTEXT 布尔、WAL 崩溃恢复）
- Phase Z 未结：M199–M210（[#352](https://github.com/tanbamboo/rusql/issues/352)–[#363](https://github.com/tanbamboo/rusql/issues/363)）
- 客户端可见面仍不完整；许多能力是文档化的 MVP 或桩
- workspace 版本在 M210 之后的 GA 切割前保持 `0.1.0`

## 阶段

### Stage 0 — 开发者预览（可并行；不可宣称即插即用）

- README / user-guide 保持 **不是生产即插即用**
- 仅在 CI 绿灯时打 tag；CHANGELOG / 双语 release-notes 标明预览
- 目的：贡献者与早期测试者可下载二进制
- **不** 完成 MySQL 8.0 终极目标

### Stage 1 — Phase Z 产品面（M199–M206）

| ID | Issue | 重点 |
|----|-------|------|
| M199 | [#352](https://github.com/tanbamboo/rusql/issues/352) | Group Replication / InnoDB Cluster — ADR 后实现子集或留下显式 blocker（禁止静默跳过） |
| M200 | [#353](https://github.com/tanbamboo/rusql/issues/353) | Clone 插件 — ADR；实现或注明替代流程 |
| M201 | [#354](https://github.com/tanbamboo/rusql/issues/354) | UDF `.so` — 显式安全跳过（已完成；ADR [adr-m201-udf-so.md](../architecture/adr-m201-udf-so.md)） |
| M202 | [#355](https://github.com/tanbamboo/rusql/issues/355) | 组件 / 插件加载器 — ADR；文档化拒绝或子集 |
| M203 | [#356](https://github.com/tanbamboo/rusql/issues/356) | 窗口命名帧 + `RANGE BETWEEN` |
| M204 | [#357](https://github.com/tanbamboo/rusql/issues/357) | Histogram / 优化器统计持久化 |
| M205 | [#358](https://github.com/tanbamboo/rusql/issues/358) | Hash join / BNL 代价 |
| M206 | [#359](https://github.com/tanbamboo/rusql/issues/359) | 临时表 `MEMORY` / InnoDB engine 子句 |

出口：全部关闭，或经 ADR + SKIPS / 兼容报告行显式标为 **Out of scope**。

### Stage 2 — 证据地板（M207–M208）

| ID | Issue | 重点 |
|----|-------|------|
| M207 | [#360](https://github.com/tanbamboo/rusql/issues/360) | 可移植 mysql-test 从 100 扩到 500 |
| M208 | [#361](https://github.com/tanbamboo/rusql/issues/361) | gap probe 成为 CI 地板（无意外缺口） |

出口：harness sensors 证明的 MySQL 面宽于精选 mysql-diff 套件。

### Stage 3 — 公开 GA 门禁（M209–M210）

| ID | Issue | 重点 |
|----|-------|------|
| M209 | [#362](https://github.com/tanbamboo/rusql/issues/362) | `rusql-vs-mysql.md` 逐项矩阵（en + zh-CN）；范围内无 **Missing** |
| M210 | [#363](https://github.com/tanbamboo/rusql/issues/363) | dump/restore + 所列 ORM + 复制 + 锁相对 Docker `mysql:8.0` 全绿；M198 ADR 已在 `main` |

出口：HANDOFF 可写终极目标达成；README 仅在有证据链接时可去掉「不是即插即用」。

### Stage 4 — 发布 GA 产物

- 仅在 M210 绿灯后切 GA 版本（不要在桩实现上发 `1.0.0`）
- GitHub Release + CHANGELOG + 中英 release-notes
- crates.io / 包仓库仅在人类批准发布密钥与命名后进行

## 运转闭环

每个 issue：[spec-to-ship](../workflows/spec-to-ship.md) → 在文件边界内实现 → sensors → 双语文档 → PR → 绿灯合并 → HANDOFF → 只给下一个序号 issue 打 `agent-ready`。

不要批量给 Phase Z 打标。在 `main` 上具备 M209、M210 证据前，不要宣称终极目标完成。

## 当前下一步产品 issue

[#352 M199](https://github.com/tanbamboo/rusql/issues/352) Group Replication：接受双语 ADR，然后实现成员子集，或留下 M210 跟踪的显式 blocker。

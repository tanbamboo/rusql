# 指标快照

摘自 [harness-retrospective-2026-06-30](../../../en/reports/harness-retrospective-2026-06-30.md)（M0–M8 窗口；同一 harness 模型延续至 M13）。

| 指标 | 数值 |
|------|------|
| M0–M13 里程碑 | `main` 上 14 个 |
| PR 首次 CI 通过 | ~87.5% |
| 分支修复 / 返工 | ~12.5%（多为 rustfmt） |
| PR 净增 LOC 中位数 | ~500 |
| 合并后用户报 bug | 回顾窗口内 0 |
| Rust 测试函数 | 50+（随 compat 增长） |

## 解读

高合并节奏、低返工说明**前馈**（Issue + ADR）与**反馈**（compat + CI）平衡。主要可重复失败：**格式** —— 成本低。

## 活指标

更新本附录时可运行 `node scripts/metrics.mjs` 获取当前 JSON 快照。

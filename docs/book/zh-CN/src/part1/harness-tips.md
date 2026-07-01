# 面向 Agent 的项目建议

rusql（M0–M13）中行之有效的模式。

## 1. 里程碑 = PR = Issue

每次合并一个垂直切片；`main` 始终可运行；用户指南始终对应当前能力。

## 2. Issue 里写决策表

涉及人的选择（认证模式、解析器）时，**先**在 Issue 中列选项，再写代码；人可通过评论异步否决。

## 3. 依赖里程碑之前写 ADR

解析器（#4）与认证（#3）ADR 使 M2–M7 无需重复争论。

## 4. Compat 夹具是最佳反馈投资

JSON 驱动真实线缆测试，能抓住单元测试漏掉的回归；SQL 对用户可见时增加夹具步骤。

## 5. 每个 P0 Issue 写文件边界

防止 Agent「好心」重构无关 crate。

## 6. 取消「是否继续？」

自主规则：取下一个 `agent-ready` Issue 或按路线图创建；人的时间留给 `needs-human`。

## 7. 每 PR 更新 CHANGELOG 与 release-notes

开发者看 CHANGELOG，用户看 release-notes；传感器校验结构（#23）。

## 8. 接受 CI rustfmt 成本

约 12.5% 分支修复率是格式 —— 推送前 `cargo fmt`。

## 9. 双语 parity 传感器

`doc-parity.mjs` 与 `check-book.mjs` 防止中英漂移。

## 10. 定期回顾指标

Harness 报告（见[附录](../appendix/metrics.md)）指导**反馈**投入，而非为流程而流程。

## Harness 启示

> 优化 **CI 变绿时间** 与 **用户可验证文档时间**，而非单次会话行数。

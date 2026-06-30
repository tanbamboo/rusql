---
name: harness-audit
description: Audit agent harness health — dead rules, missing sensors, doc rot, and HARNESS_CHANGELOG gaps. Use when reviewing harness quality or monthly maintenance.
---

# Harness Audit

审计项目 harness 健康度，输出可操作的改进清单。

## 何时使用

- 每月定期维护
- Agent 反复犯同类错误
- 大版本迁移后
- 新成员 onboarding 前

## 审计清单

### 1. 结构完整性

运行 `pnpm harness:validate`，确认通过。

检查 `anr.yaml` 中 `required_files` 和 `required_directories` 均存在。

### 2. Guide 质量

- [ ] `AGENTS.md` < 200 行，链接无断链
- [ ] 每条 always-apply rule 有对应 sensor 或明确理由
- [ ] `profiles/*/guides.md` 与当前栈版本一致
- [ ] 无相互矛盾的 rules

### 3. Sensor 覆盖

- [ ] 本地命令与 CI 一致
- [ ] 每个 profile 的 `sensors.yaml` 命令可执行
- [ ] pre-commit 覆盖最快检查
- [ ] 架构边界有自动化验证

### 4. 进化记录

- [ ] `HARNESS_CHANGELOG.md` 近 30 天有更新（若 agent 活跃）
- [ ] 重复失败模式已升级为 sensor 而非重复 prose

### 5. 文档卫生

- [ ] `HANDOFF.md` 反映当前状态
- [ ] 过时 spec 已归档或删除
- [ ] `.cursorignore` 覆盖 `node_modules`、dist、.venv

## 输出格式

```markdown
## Harness Audit — YYYY-MM-DD

### 通过项
- ...

### 待改进（按优先级）
1. [P0] ...
2. [P1] ...

### 建议的 HARNESS_CHANGELOG 条目
- ...
```

## 参考

- [harness-evolution.md](../../docs/workflows/harness-evolution.md)
- [HARNESS_CHANGELOG.md](../../HARNESS_CHANGELOG.md)

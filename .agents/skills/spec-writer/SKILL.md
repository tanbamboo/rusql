---
name: spec-writer
description: Write feature specifications for AI-native development with acceptance criteria, file boundaries, and negative constraints. Use when starting a new feature or user story.
---

# Spec Writer

为 AI-native 开发编写高质量功能规格。

## 何时使用

- 新功能开发前
- Issue 转 agent 任务前
- Plan 模式输入准备

## 规格模板

在 `docs/specs/<feature-name>.md` 创建：

```markdown
# <Feature Name>

## 目标
一句话描述要达成什么。

## 背景
为什么需要这个功能。

## 验收标准
- [ ] 可测试的标准 1
- [ ] 可测试的标准 2

## 文件边界
允许修改：
- `packages/web/src/...`
- `packages/shared/src/...`

禁止修改：
- `packages/api/...`
- `.github/...`

## 负面约束
- 不引入新外部依赖
- 不改变现有 API 契约

## 技术说明
（可选）实现提示、需沿用的现有抽象。

## 测试要求
- 单元测试覆盖 ...
- 手动验证步骤 ...

## 风险与依赖
- 依赖 issue #...
- 风险：...
```

## 质量检查

规格合格当且仅当：
1. 验收标准可自动化或手动明确验证
2. 文件边界可 glob 匹配
3. 负面约束明确
4. 无模糊词（「更好」「优化」而无度量）

## 参考

- [spec-to-ship.md](../../docs/workflows/spec-to-ship.md)
- [CONSTITUTION.md](../../CONSTITUTION.md) — Spec-first 原则

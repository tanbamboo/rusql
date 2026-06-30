---
name: pr-reviewer
description: Review pull requests for AI-generated code. Must ground findings in CI/sensor output before semantic analysis. Use for PR review or pre-merge checks.
---

# PR Reviewer

对 AI 生成代码进行结构化审查。**必须先引用 sensor 输出，再做语义判断。**

## 审查顺序

### 1. Sensor 输出（强制）

读取并总结：
- CI 状态
- lint / typecheck / test 结果
- `pnpm harness:validate` 结果

若 sensor 未全绿 → **Blocking**：不得建议合并。

### 2. Spec 对照

- 验收标准是否满足？
- 变更是否在文件边界内？
- 负面约束是否违反？

### 3. 架构

对照 [boundaries.md](../../docs/architecture/boundaries.md)：
- 依赖方向正确？
- 无跨包违规 import？

### 4. 安全扫视

- 无硬编码密钥
- auth 路径变更是否合理
- 用户输入是否验证

### 5. 语义质量（仅在前 4 步通过后）

- 过度工程化？
- 误解业务意图？
- 测试是否验证行为而非实现细节？

## 输出格式

```markdown
## PR Review

### Sensor Summary
- CI: pass/fail
- Tests: X passed, Y failed
- Lint: ...

### Blocking
- [ ] 问题描述 → 建议修复

### Suggestions
- 非阻塞改进建议

### Nits
- 风格偏好（可选修）
```

## 禁止

- 在未读 CI 输出时猜测问题
- 重复 lint 已捕获的问题
- 建议超出 spec 范围的「顺便改进」

## 参考

- [pr-review.md](../../.agents/workflows/pr-review.md)
- [trust.md](../../docs/agent-governance/trust.md)

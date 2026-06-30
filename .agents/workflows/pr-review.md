# PR 审查工作流

## Agent 自检（合并前必做）

1. 运行 profile sensors 全绿
2. 确认变更在 spec 文件边界内
3. 更新相关文档或声明无文档影响
4. 填写 PR 模板

## Reviewer Subagent 流程

使用 `.cursor/agents/reviewer.md` 或 pr-reviewer skill：

1. **先读 sensor 输出**——lint、test、CI 结果
2. **对照 spec**——验收标准是否满足
3. **架构检查**——是否违反 [boundaries.md](../../docs/architecture/boundaries.md)
4. **安全扫一眼**——密钥、auth、SQL 注入面
5. **输出结构化 review**——Blocking / Suggestion / Nit

## 人类审查聚焦

Harness 应已捕获的问题，人类不再重复审查：
- 格式、lint、类型错误
- 明显缺失的测试（若 CI 已覆盖）

人类应聚焦：
- 业务逻辑正确性
- API 设计权衡
- 安全与合规
- 与团队路线图一致性

## 合并条件

- [ ] CI 全绿
- [ ] 至少 1 人类 approval（T1+ 路径需 CODEOWNERS）
- [ ] PR 模板完整
- [ ] HANDOFF.md 已更新（若属活跃 sprint）

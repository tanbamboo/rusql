# 密钥与凭证政策

## 禁止

- 在仓库中存储 API key、密码、token、私钥
- 在 PR、issue、日志中粘贴凭证
- Agent 创建 `.env` 并提交（`.env` 必须在 `.gitignore`）

## 允许

- `.env.example` 含占位符和说明
- 文档中引用环境变量**名称**（非值）
- CI 使用 GitHub Secrets / 密钥管理服务

## 检测

- pre-commit：基础密钥模式扫描
- CI：gitleaks 或等价工具（团队 fork 后启用）
- Hook：`before-shell-guard` 拦截明显密钥写入

## Agent 指令

1. 需要凭证时，停止并提示人类配置环境变量
2. 使用 `process.env.VAR_NAME` / `os.environ["VAR"]` 模式
3. 在 README 中记录所需环境变量名称

## 泄露响应

1. 立即轮换泄露的凭证
2. 从 git 历史清除（需人类执行）
3. 记录到 HARNESS_CHANGELOG 并加强 sensor

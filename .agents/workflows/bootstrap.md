# Bootstrap 工作流

将模板定制为团队项目的步骤。

## 前置条件

- Git
- Node.js 18+ 和 pnpm
- （可选）Python 3.11+、.NET 8+

## 步骤

### 1. Fork / Clone

```bash
git clone <your-repo-url>
cd ai-native-harness-template
```

### 2. 运行 Bootstrap

```powershell
# Windows
.\scripts\bootstrap.ps1 -Profile typescript -ProjectName "my-product"

# Unix
./scripts/bootstrap.sh --profile typescript --project-name "my-product"
```

### 3. 填写项目信息

- [ ] 更新 `AGENTS.md` 项目目的
- [ ] 更新 `docs/architecture/overview.md`
- [ ] 更新 `README.md`
- [ ] 配置 `anr.yaml` 的 `active_profile`

### 4. 安装依赖

```bash
pnpm install
```

### 5. 验证 Harness

```bash
pnpm harness:validate
```

### 6. 首次 Spec-to-Ship

用 Cursor Plan 模式完成一个小功能，走通 [spec-to-ship.md](../../docs/workflows/spec-to-ship.md) 全流程。

## 添加新 Package

1. 在 `packages/` 下创建目录
2. 选择 profile，复制 `profiles/<profile>/templates/` 脚手架
3. 更新 `anr.yaml` 的 `packages` 列表
4. 更新本 context-index 和根 `AGENTS.md` 地图

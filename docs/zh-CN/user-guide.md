# 用户指南 — 测试 rusql

说明 **main 分支当前可用功能** 及验证方法。

英文 canonical：[user-guide.md](../en/user-guide.md)

## 前置条件

- Rust 1.75+（[rustup](https://rustup.rs)）
- 可选：MySQL 客户端（`mysql` CLI）用于手动测试

## 构建与运行

```bash
cargo build --release
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data
```

- `--data-dir` — WAL 文件目录（`rusql.wal`），默认 `rusql-data`
- **重启后数据仍在**：停止服务再启动，表和行会从 WAL 重放

### 可选密码验证

默认握手插件为 `caching_sha2_password`（MySQL 8）。`--auth-password` 启用密码校验。

```bash
cargo run -p rusql-server -- --port 3307 --auth-password 你的密码
```

用户默认为 `root`，可用 `--auth-user` 修改。详见 [adr-m6-auth-and-dml.md](../en/specs/adr-m6-auth-and-dml.md)。

中文错误消息：

```bash
RUSQL_LOCALE=zh-CN cargo run -p rusql-server -- --port 3307
```

## 自动化测试

```bash
cargo test -p rusql-server compat
cargo test -p rusql-server com_query
cargo test
```

兼容性 JSON 用例位于 `crates/rusql-server/compat/`。

## MySQL 客户端

```bash
mysql -h 127.0.0.1 -P 3307 -u root --protocol=TCP
```

```sql
CREATE TABLE users (id INT, name VARCHAR(64));
INSERT INTO users VALUES (1, 'alice');
BEGIN;
INSERT INTO users VALUES (2, 'bob');
COMMIT;
DELETE FROM users WHERE id = 1;
DROP TABLE users;
```

## 已实现功能（M1–M9）

| 功能 | 状态 | 说明 |
|------|------|------|
| 握手 + 可选密码验证 | 完成 | `--auth-password` |
| COM_QUERY / COM_QUIT | 完成 | |
| CREATE / INSERT / SELECT | 完成 | |
| SELECT 列列表 | 完成 | M14 |
| DROP / DELETE / UPDATE | 完成 | |
| 事务 | 完成 | `BEGIN` / `COMMIT` / `ROLLBACK` |
| SHOW TABLES / DATABASES | 完成 | M10 元数据发现 |
| DESCRIBE / information_schema | 完成 | M12 表结构发现 |
| SHOW CREATE TABLE | 完成 | M13 DDL 导出 |
| 预编译语句 | 完成 | M11 `COM_STMT_*` |
| 持久化、索引、兼容性测试套件 | 完成 | `cargo test -p rusql-server compat` |

## 持久化测试

```bash
cargo test -p rusql-server persistence_across_connections
```

## 故障排查

| 问题 | 处理 |
|------|------|
| 连接被拒绝 | 确认服务已启动且端口一致 |
| 认证插件错误 | 尝试 `--default-auth=mysql_native_password` |

## 开发传感器

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
node scripts/doc-parity.mjs
node scripts/check-changelog.mjs
node scripts/metrics.mjs
```

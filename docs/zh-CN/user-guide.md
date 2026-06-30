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

中文错误消息：

```bash
RUSQL_LOCALE=zh-CN cargo run -p rusql-server -- --port 3307
```

## 自动化测试

```bash
cargo test -p rusql-server com_query
cargo test -p rusql-server persistence_across_connections
cargo test
```

## MySQL 客户端

```bash
mysql -h 127.0.0.1 -P 3307 -u root --default-auth=mysql_native_password --protocol=TCP
```

```sql
CREATE TABLE users (id INT, name VARCHAR(64));
CREATE INDEX idx_users_id ON users (id);
INSERT INTO users VALUES (1, 'alice');
SELECT * FROM users WHERE id = 1;
```

重启服务后再次执行 `SELECT * FROM users WHERE id = 1;`，数据应仍在。

## 已实现功能（M1–M4）

| 功能 | 状态 | 说明 |
|------|------|------|
| MySQL wire protocol v10 握手 | 完成 | `mysql_native_password` 桩（尚未校验密码） |
| COM_QUERY | 完成 | 单条 SQL |
| COM_QUIT | 完成 | |
| CREATE TABLE | 完成 | |
| INSERT … VALUES | 完成 | |
| SELECT * FROM table | 完成 | |
| SELECT 字面量 | 完成 | 如 `SELECT 1` |
| 持久化（WAL） | 完成 | `--data-dir`，文件 `rusql.wal` |
| 预编译语句 | 未实现 | |
| 事务 | 未实现 | |
| 索引 | 完成 | `CREATE INDEX`，`WHERE col = literal` 点查 |

## 开发传感器

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
node scripts/harness-validate.mjs
```

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

-- GRANT / REVOKE（M54）
GRANT SELECT, INSERT ON rusql.* TO app;
SHOW GRANTS FOR app;

-- 复合索引（M50）
CREATE INDEX idx_ab ON t (a, b);
SELECT * FROM t WHERE a = 1 AND b = 2;

-- 多用户认证（M55-auth）
CREATE USER 'app'@'%' IDENTIFIED BY 'secret';
CREATE USER 'legacy'@'%' IDENTIFIED WITH mysql_native_password BY 'secret';
DROP USER 'legacy'@'%';
```

## 已实现功能（M1–M9）

| 功能 | 状态 | 说明 |
|------|------|------|
| 握手 + 可选密码验证 | 完成 | `--auth-password` |
| COM_QUERY / COM_QUIT | 完成 | |
| CREATE / INSERT / SELECT | 完成 | |
| SELECT 列列表 | 完成 | M14 |
| ORDER BY | 完成 | M17 |
| 列别名 | 完成 | M18 `SELECT col AS alias` |
| LIMIT | 完成 | M16 |
| OFFSET | 完成 | M19 `LIMIT n OFFSET m` |
| DROP / DELETE / UPDATE | 完成 | |
| 事务 | 完成 | `BEGIN` / `COMMIT` / `ROLLBACK` |
| SHOW TABLES / DATABASES | 完成 | M10 元数据发现 |
| DESCRIBE / information_schema | 完成 | M12 表结构发现 |
| SHOW CREATE TABLE | 完成 | M13 DDL 导出 |
| 预编译语句 | 完成 | M11 `COM_STMT_*` |
| COM_CHANGE_USER / COM_RESET_CONNECTION | 完成 | M51 重新认证；重置清除预编译状态 |
| COM_FIELD_LIST / 长参数 | 完成 | M52 字段列表；`COM_STMT_SEND_LONG_DATA` + `COM_STMT_RESET` |
| SHOW PROCESSLIST / COM_PROCESS_INFO | 完成 | M53 连接注册表 |
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

## 性能基准（PERF-B1）

持久连接微基准（与 [performance-benchmark-2026-08-11.md](../en/reports/performance-benchmark-2026-08-11.md) 相同的 7 项 workload）：

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./.test-data-bench
node scripts/bench-rusql-vs-mysql.mjs --host 127.0.0.1 --port 3307 --label rusql --output target/bench-rusql.json
```

### 多线程基准（PERF-B4）

```bash
node scripts/bench-rusql-vs-mysql.mjs --threads 8 --duration 30 --workloads read-heavy \
  --host 127.0.0.1 --port 3307 --label rusql
node scripts/bench-rusql-vs-mysql.mjs --thread-matrix --compare --rusql-port 3307 --mysql-port 3308
```

### WAL 同步策略（PERF-B5）

```bash
cargo run -p rusql-server -- --wal-sync always --port 3307 --data-dir ./.test-data-bench  # 默认
cargo run -p rusql-server -- --wal-sync batch --port 3307 --data-dir ./.test-data-bench
cargo run -p rusql-server -- --wal-sync none --port 3307 --data-dir ./.test-data-bench
```

**警告**：`batch` 与 `none` 以持久性换吞吐，仅用于基准或允许崩溃丢数据的场景。

### Sysbench 门禁（PERF-B6）

```bash
node scripts/sysbench-rusql.mjs --rusql-port 3307 --mysql-port 3308 --threads 8 --time 30
```

需安装 Sysbench 与 Docker MySQL；工具缺失时软失败（exit 0）。

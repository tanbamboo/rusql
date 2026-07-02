# 版本说明

`main` 分支已合并功能的用户向摘要。日常使用见 [user-guide.md](user-guide.md)。

**English**: [release-notes.md](../en/release-notes.md)

---

## 最新：M25 — 二进制结果集 COM_STMT_EXECUTE（2026-06-30）

**内容**：预编译 SELECT 返回二进制协议行，列类型正确（`INT` 为 4 字节小端，`VARCHAR` 为 lenenc 字符串）。

```bash
cargo test -p rusql-protocol binary
cargo test -p rusql-server stmt_prepare_execute_binary
```

---

## M24 — ALTER TABLE ADD COLUMN（2026-06-30）

**内容**：`ALTER TABLE t ADD COLUMN c TYPE`（及 MySQL 简写 `ADD c TYPE`）；已有行新列为 NULL（空串）；WAL 重放。

```bash
cargo test -p rusql-executor alter_table_add_column
cargo test -p rusql-server compat
```

---

## M19 — SELECT LIMIT OFFSET（2026-06-30）

**内容**：`LIMIT n OFFSET m`，与 ORDER BY 组合分页。

```sql
SELECT * FROM users ORDER BY id LIMIT 1 OFFSET 1;
```

---

## M18 — SELECT 列别名（2026-06-30）

**内容**：`SELECT id AS user_id` 等，`AS` 别名作为结果集列名。

```sql
SELECT id AS user_id FROM users;
```

---

## M17 — SELECT ORDER BY（2026-06-30）

**内容**：表 `SELECT` 支持 `ORDER BY col [ASC|DESC]`（在投影/过滤之后、`LIMIT` 之前）。

```sql
SELECT * FROM users ORDER BY id;
SELECT name FROM users ORDER BY name DESC;
```

```bash
cargo test -p rusql-executor select_order_by
cargo test -p rusql-server compat
```

---

## M16 — SELECT LIMIT（2026-06-30）

**内容**：`SELECT * FROM tbl LIMIT n` 限制返回行数。

---

## M15 — USE database（2026-06-30）

**内容**：`USE rusql` 设置会话默认数据库。

```sql
USE rusql;
```

---

## 最新：M14 — SELECT 列投影（2026-06-30）

**内容**：`SELECT id, name FROM users` 仅返回所列列；`SELECT *` 行为不变。

```sql
SELECT name FROM users;
```

```bash
cargo test -p rusql-executor select_column_projection
```

---

## 书籍 — Harness Engineering 叙事（#28）

**内容**：中英 mdBook —— 每里程碑一章（M0–M13）、Harness 专篇、指标附录。

**阅读**：[docs/book/README.md](../../docs/book/README.md)

```bash
cargo install mdbook
node scripts/build-book.mjs
node scripts/check-book.mjs
```

---

## M13 — SHOW CREATE TABLE（2026-06-30）

**内容**：`SHOW CREATE TABLE tbl` 返回可重建的 DDL 字符串。

```sql
SHOW CREATE TABLE users;
```

```bash
cargo test -p rusql-executor show_create
```

---

## M12 — DESCRIBE 与 information_schema（2026-06-30）

**内容**：`DESCRIBE tbl`、`SHOW COLUMNS FROM tbl`，以及虚拟表 `information_schema.tables` / `information_schema.columns`。

```sql
DESCRIBE users;
SELECT * FROM information_schema.tables;
SELECT * FROM information_schema.columns WHERE table_name = 'users';
```

```bash
cargo test -p rusql-executor describe
cargo test -p rusql-server describe
cargo test -p rusql-server run_basic_compat_fixtures
```

规范：[m12-describe-info-schema.md](../en/specs/m12-describe-info-schema.md)

---

## M11 — 预编译语句（2026-06-30）

**内容**：`COM_STMT_PREPARE` / `EXECUTE` / `CLOSE`，支持 `?` 占位符。

```bash
cargo test -p rusql-server stmt_prepare
```

---

## M10 — SHOW TABLES / SHOW DATABASES（2026-06-30）

**内容**：列出当前库中的表及默认数据库名 `rusql`。

```sql
SHOW TABLES;
SHOW DATABASES;
```

```bash
cargo test -p rusql-server compat
```

另见根目录 `CHANGELOG.md` 与 PR 合并后的更新约定（Issue #23）。

---

## M9 — 事务（2026-06-30）

**内容**：`BEGIN`、`COMMIT`、`ROLLBACK`；未提交数据仅当前连接可见。

```bash
cargo run -p rusql-server -- --port 3307
```

```sql
CREATE TABLE t (id INT);
BEGIN;
INSERT INTO t VALUES (1);
COMMIT;
```

```bash
cargo test -p rusql-server transaction_commit_and_rollback
```

---

## M8 — UPDATE

`UPDATE … SET … WHERE`，WAL 持久化。

## M7 — caching_sha2_password

MySQL 8 默认认证插件；`--auth-password` 可选启用校验。

## M6 — 认证与 DROP / DELETE

## M5 — 兼容性 JSON 测试套件

```bash
cargo test -p rusql-server compat
```

## M3 — 持久化

`--data-dir`，重启后 WAL 重放。

## M2 — COM_QUERY

## M1 — 握手

## M0 — Harness 工程化

```bash
node scripts/harness-validate.mjs
node scripts/metrics.mjs
```

---

## 更新约定

每次合并影响用户行为的 PR 须：

1. 更新根目录 `CHANGELOG.md` 的 `[Unreleased]`
2. 更新本文件「最新」小节
3. 按需更新 [user-guide.md](user-guide.md)

校验：`node scripts/check-changelog.mjs`

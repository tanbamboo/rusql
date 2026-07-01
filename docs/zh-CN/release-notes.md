# 版本说明

`main` 分支已合并功能的用户向摘要。日常使用见 [user-guide.md](user-guide.md)。

**English**: [release-notes.md](../en/release-notes.md)

---

## 最新：M11 — 预编译语句（2026-06-30）

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

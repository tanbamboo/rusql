# ADR：M187 `FLUSH PRIVILEGES`

**状态**：已接受（M187 / [#339](https://github.com/tanbamboo/rusql/issues/339)）  
**日期**：2026-10-27

## 背景

`CREATE USER`、`GRANT`、`REVOKE`、角色变更和密码变更已经在返回 OK 之前更新内存中的权限目录，并写入 `{data_dir}/mysql.user.json`。直接改这个文件在进程再次读取之前不可见。MySQL 的 `FLUSH PRIVILEGES` 就是这次读取：从持久化存储重新加载授权表。不读文件就返回 OK 是错误的，即便 `GRANT` 已经在内存中生效也一样。

重新加载与认证读取的是同一份目录。未决问题是这会不会与登录竞争。本 ADR 决定锁顺序。它不修改 `CONSTITUTION.md`。企业插件仍属于 M188。

## 决定

1. **`FLUSH PRIVILEGES` 重新加载文件。** `FLUSH PRIVILEGES`、`FLUSH LOCAL PRIVILEGES` 和 `FLUSH NO_WRITE_TO_BINLOG PRIVILEGES`（以及仅含 `PRIVILEGES` 的逗号列表）取得权限目录的写锁，读取 `mysql.user.json`，并用该快照替换内存目录。语句随后返回 OK。它不与内存合并，也不写文件。已经把文件写盘的 `GRANT` 或 `CREATE USER` 仍然有效，因为重新加载读的是同一文件。若文件在那之后被替换，以文件为准，下一次连接看到替换后的账号和授权。

2. **目录不会撕裂。** 认证在持有读锁时复制账号密码，然后释放锁。重新加载在整个读取并替换期间持有写锁。因此一次登录看到的要么是上一份目录，要么是完整解析后的替换结果。解析失败（errno **1105**）保留上一份目录。文件缺失则加载空目录，与进程启动时没有该文件相同。进程内的账号 DDL 在 OK 之前写文件，因此随后的 `FLUSH PRIVILEGES` 不会清掉这些账号，除非文件本身被删除或替换。

3. **谁可以执行。** rusql 没有单独的 `RELOAD` 权限位。`root` 是代替，与其他账号管理门禁一致。其他账号得到 errno **1227**（`ER_SPECIFIC_ACCESS_DENIED_ERROR`），沿用该 errno 现有的 rusql 措辞，并写明 `RELOAD`。该检查适用于每一种已识别的 `FLUSH` 形式，且发生在重新加载之前。密码过期仍然是 errno **1820**，不是一次 flush。

4. **其他 `FLUSH` 形式不执行。** `FLUSH TABLES`（含表清单和 `WITH READ LOCK`）、`FLUSH HOSTS`、`FLUSH STATUS`、`FLUSH LOGS`、`FLUSH BINARY LOGS`、`FLUSH RELAY LOGS`、`FLUSH USER_RESOURCES`、`FLUSH OPTIMIZER_COSTS` 以及各类日志选项，对 `root` 是 errno **1235**（`ER_NOT_SUPPORTED_YET`）。它们不关闭表、不加读锁、不删除数据库、不刷新主机缓存，也不重新加载 TLS 证书。`PRIVILEGES` 与其他选项混在同一列表中是 **1235**，并且不重新加载。无法识别的 `FLUSH` 语法对任何账号都是 errno **1064**。不实现 `FLUSH HOSTS`。

5. **`SELECT mysql.user` 读同一份目录。** 重新加载成功后，`SELECT mysql.user`（M186）和之后的登录使用替换后的快照。密码哈希仍留在文件中，仍然不是结果列。

## 后果

- 在磁盘上修改 `mysql.user.json` 要到 `FLUSH PRIVILEGES` 或进程重启才生效。当前进程里的 SQL 账号语句不需要 `FLUSH`，而 `FLUSH` 仍会重新读取文件。
- `FLUSH TABLES` 和 `FLUSH TABLES WITH READ LOCK` 在 rusql 上以 errno **1235** 失败，在 MySQL 8.0 上成功。`mysql-diff` 覆盖 `FLUSH PRIVILEGES`（含 `LOCAL` / `NO_WRITE_TO_BINLOG`），不比较那些其他形式。
- 非超级用户的 `FLUSH` 是 errno **1227**，包括 `root` 会得到 **1235** 的形式。
- 本切片不查阅 `read_only` / `super_read_only`。

## 范围外

- `FLUSH TABLES`、`FLUSH TABLES WITH READ LOCK`，以及关闭或删除数据库
- `FLUSH HOSTS`、`FLUSH STATUS`，以及日志 / 优化器 / 用户资源形式
- 与 `root` 分离的可授予 `RELOAD` 权限
- 重新加载 TLS 证书（M181）

## 如何验证

```bash
cargo test -p rusql-executor flush_privileges
cargo test -p rusql-server flush_privileges
```

```sql
FLUSH PRIVILEGES;
```

用包含新账号的目录替换 `{data_dir}/mysql.user.json`，以 `root` 执行 `FLUSH PRIVILEGES`，再以该账号连接。`root` 执行 `FLUSH TABLES` 是 errno **1235**。其他用户执行同一语句是 errno **1227**。

# ADR：M186 `mysql.user` 表形状

**状态**：已接受（M186 / [#338](https://github.com/tanbamboo/rusql/issues/338)）  
**日期**：2026-10-27

## 背景

连接器会 `SELECT mysql.user` 来探测账号（host、plugin、`ssl_type`）。现在这个名字不是表，语句是 errno **1146**。账号已经在 `{data_dir}/mysql.user.json` 里（M55，以及 M182 角色图、M183 密码字段、M184 `require_ssl`）。把该 JSON 文件交给客户端会暴露密码哈希，而且那也不是一张 SQL 表。

本 ADR 是这些探测使用的可查询列子集。不修改 `CONSTITUTION.md`。`FLUSH PRIVILEGES` 仍属于 M187。企业插件仍属于 M188。

## 决策

1. **虚拟表，不是文件转储。** 对 `mysql.user` 的 `SELECT`（大小写不限，包括 `USE mysql` 之后 `SELECT … FROM user`）从权限目录读取登录账号。它不是基表，也不是 `mysql.user.json` 的内容。`INSERT`、`UPDATE`、`DELETE` 仍然看不到基表（errno **1146**）。其他 `mysql.*` 名字，例如 `mysql.db`，仍是 errno **1146**。

2. **列与 MySQL 8.0 类型。** 只有探测子集。`SELECT *` 按此顺序返回：

   | 列 | MySQL 8.0 类型 | 值 |
   |----|----------------|----|
   | `Host` | `char(255)` | 账号 host |
   | `User` | `char(32)` | 账号名 |
   | `plugin` | `char(64)` | `auth_plugin`（`caching_sha2_password` 或 `mysql_native_password`） |
   | `ssl_type` | `enum('','ANY','X509','SPECIFIED')` | 账号有 M184 `require_ssl` 时为 `ANY`，否则为空字符串 |

   行是登录账号，按 `Host`、再按 `User` 排序。空目录返回这些列和零行，不是 errno 1146。M182 的角色不是行。

   执行器里“不要求 SSL”的单元格是空字符串。rusql 的文本协议把每个空字符串编码成 SQL NULL，因此客户端在该单元格看到 NULL，在设置了 `require_ssl` 时看到字符串 `ANY`。本切片只产生 `''` 和 `ANY`。

3. **没有密码列。** `authentication_string` 不是列。`SELECT authentication_string FROM mysql.user` 是 errno **1054**（`ER_BAD_FIELD_ERROR`）。子集以外的其他名字（`Select_priv`、`account_locked`，以及 8.0 表的其余列）同样是这个 errno。哈希留在 `mysql.user.json` 里，不会复制进结果。`X509` 和 `SPECIFIED` 是类型里的合法枚举值，但不会产生：M184 用 errno **1064** 拒绝那些 `REQUIRE` 形式。

4. **权限。** `root` 可以 `SELECT`。其他账号需要对 `mysql.user` 的 `SELECT`（或更宽的授权）。没有授权时语句是 errno **1142**。授权也不会把密码哈希放进行里。

## 后果

- `SELECT Host, User, plugin` 或 `ssl_type` 的客户端能看到目录账号。期待完整 8.0 列（包括 `authentication_string`）的客户端，在缺失的列名上得到 errno **1054**。
- `mysql-diff` 对没有 `REQUIRE SSL` 的账号比较 `Host`、`User`、`plugin`，在值为 `ANY` 时才带上 `ssl_type`。它不查询 `authentication_string`，因此输出里没有哈希。空的 `ssl_type` 不参与比较，因为线上把它发成 NULL。
- 在磁盘上改 `mysql.user.json` 要等进程重新加载目录才可见。重新加载是 M187（`FLUSH PRIVILEGES`），不是本切片。

## 不在范围内

- 完整的 MySQL 8.0 `mysql.user` 列清单和可写授权表
- `mysql.db`、`mysql.tables_priv` 以及其他权限表
- 返回或脱敏 `authentication_string`（该列不存在）
- 把角色当成 `mysql.user` 的行
- `FLUSH PRIVILEGES`（M187）

## 如何验证

```bash
cargo test -p rusql-executor mysql_user_table
```

```sql
CREATE USER 'app'@'%' IDENTIFIED WITH caching_sha2_password BY 'secret';
CREATE USER 'appssl'@'%' IDENTIFIED WITH caching_sha2_password BY 'secret' REQUIRE SSL;
SELECT Host, User, plugin, ssl_type FROM mysql.user WHERE User = 'appssl';
```

`appssl` 行的 `plugin` 为 `caching_sha2_password`，`ssl_type` 为 `ANY`。`SELECT authentication_string FROM mysql.user` 是 errno **1054**。`SELECT Host FROM mysql.db` 是 errno **1146**。

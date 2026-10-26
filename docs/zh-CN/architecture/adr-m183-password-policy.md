# ADR：M183 密码策略（`PASSWORD EXPIRE` / 校验 / 重用）

**状态**：已接受（M183 / [#335](https://github.com/tanbamboo/rusql/issues/335)）  
**日期**：2026-10-26

## 背景

MySQL 8.0 可以把 `caching_sha2_password` 账号标记为下次会话必须修改密码。登录仍然成功。在 `ALTER USER` 或 `SET PASSWORD` 设置新密码之前，除改密码以外的语句都失败，errno **1820**（`ER_MUST_CHANGE_PASSWORD`）。账号还可以有重用限制（`PASSWORD HISTORY`、`PASSWORD REUSE INTERVAL`）和有效期（`PASSWORD EXPIRE INTERVAL` / `default_password_lifetime`）。安装了 `validate_password` 组件时，弱密码以 errno **1819** 拒绝。

rusql 已经校验 `caching_sha2_password` 与 `mysql_native_password`（M7 / M55），并把账号存在 `{data_dir}/mysql.user.json`。本 ADR 是 M183 的子集。不修改 `CONSTITUTION.md`。账号级 `REQUIRE SSL` 仍属于 M184。

## 决策

1. **过期即必须修改。** `ALTER USER user PASSWORD EXPIRE` 与 `CREATE USER … PASSWORD EXPIRE` 在账号行上设置 `password_expired`。密码正确时握手仍然完成。随后的 `COM_QUERY`（以及 `COM_INIT_DB`、字段列表、预处理语句、进程信息）返回 errno **1820**，直到该账号设置新密码。`COM_PING` 仍然可用。修改密码会清除该标志，除非同一条语句同时写了 `PASSWORD EXPIRE`。

2. **谁可以改密码。** 超级用户（`root`）可以 `CREATE` / `DROP` / `ALTER` 账号，也可以 `SET PASSWORD FOR` 任何人。任何账号都可以用 `SET PASSWORD`、`SET PASSWORD FOR USER()` 或 `ALTER USER USER() IDENTIFIED BY`（或自己的 `user`@`host`）修改自己的密码，前提是这条语句不同时修改策略。策略子句（`PASSWORD EXPIRE`、`PASSWORD HISTORY`、`PASSWORD REUSE INTERVAL`）仍走超级用户门槛（errno **1227**）。账号不存在是 errno **1396**。

3. **有效期。** `PASSWORD EXPIRE NEVER`、`PASSWORD EXPIRE DEFAULT` 与 `PASSWORD EXPIRE INTERVAL n DAY` 存在账号上。`DEFAULT` 使用全局 `default_password_lifetime`（默认 **0**，表示不按时间过期，与 MySQL 8.0 一致）。`NEVER` 不按时间过期，也不会清除已经过期的标志。有效期非 0 时，若 `now >= password_changed_unix + days * 86400`，账号过期。修改时间为 `0`（M183 之前的目录）不按时间过期。`SET GLOBAL default_password_lifetime = n` 写入 `mysql.user.json`。

4. **重用记在账号行上。** 没有 `mysql.password_history` 表。以前的密码按从新到旧存在同一条账号记录里，表示方式与 M55 当前密码相同（不改变哈希）。全局 `password_history` 与 `password_reuse_interval` 默认为 **0**（不检查）。`ALTER USER … PASSWORD HISTORY n` 与 `PASSWORD REUSE INTERVAL n DAY` 覆盖该账号；`DEFAULT` 跟随全局值。有效条数为 N > 0 时，新密码不得与当前密码或最新的 N−1 条保留值相同（errno **3638**）。重用间隔 D > 0 时，也不得与修改时间距今不足 D 天的保留密码相同。两个限制同时生效。历史最多保留 64 条。

5. **`validate_password` 是内置的，默认关闭。** 没有插件 `.so`，也没有 `INSTALL COMPONENT`（M201）。`validate_password.policy` 默认为 `OFF`，因此 `CREATE USER` / `ALTER USER` / `SET PASSWORD` 接受与现在相同的密码，包括 `'secret'` 这类夹具。`SET GLOBAL validate_password.policy = LOW|MEDIUM|STRONG`（或 `0|1|2`）打开检查。`LOW` 要求不少于 `validate_password.length` 个字符（默认 8；`0` 表示不检查长度）。`MEDIUM` 还要求各有一个 ASCII 小写、大写、数字和非字母数字字符。`STRONG` 是 `MEDIUM`，并且当账号名至少 3 个字符且密码包含该账号名时拒绝。没有词典文件。失败是 errno **1819**。不带 `GLOBAL` / `@@GLOBAL` 的 `SET` 是 errno **1229**。非法取值是 errno **1231**。

6. **读取。** `SELECT @@validate_password.length`、`@@validate_password.policy`、`@@password_history`、`@@password_reuse_interval` 与 `@@default_password_lifetime`（可加 `GLOBAL.` / `SESSION.` 前缀）返回全局值。`SHOW VARIABLES` 不列出它们（stub 目录不变）。

7. **未过期的登录不变。** 未过期的账号仍用 `caching_sha2_password` 或 `mysql_native_password` 认证，并且可以 `COM_QUERY`。哈希函数不修改。旧的 `mysql.user.json` 在新字段缺省时仍能加载。

## 后果

- 过期账号可以连上，然后只能修改自己的密码，直到修改成功。
- 把策略设为 `LOW` 且长度为 8 会拒绝现有测试里的短密码。默认保持 `OFF`，那些语句仍然成功。
- 复制数据目录会把过期、历史和全局策略与账号文件一起复制走。历史项与 M55 存储的密码字符串相同。不要把它们写进日志。

## 不在范围内

- `PASSWORD REQUIRE CURRENT` / `REPLACE`（errno 3891–3893）
- `STRONG` 的词典文件
- 可加载的 `validate_password` `.so`（M201）
- 超出账号行上限的双密码 / 保留天数
- 账号级 `REQUIRE SSL`（M184）

## 如何验证

```bash
cargo test -p rusql-protocol password_expire
cargo test -p rusql-server password_expire
```

手工：

```sql
CREATE USER 'app'@'%' IDENTIFIED BY 'secret';
ALTER USER 'app'@'%' PASSWORD EXPIRE;
-- 以 app 连接；SELECT 1 是 errno 1820
ALTER USER USER() IDENTIFIED BY 'secret2';
SELECT 1;  -- 成功
```

`SET GLOBAL validate_password.policy = LOW` 之后，`CREATE USER … IDENTIFIED BY 'short'` 是 errno **1819**。

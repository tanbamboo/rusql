# ADR：M184 账号级 `REQUIRE SSL`

**状态**：已接受（M184 / [#336](https://github.com/tanbamboo/rusql/issues/336)）  
**日期**：2026-10-27

## 背景

MySQL 8.0 可以只要求某一个账号使用加密连接（`CREATE USER` / `ALTER USER … REQUIRE SSL`），而不打开进程级 `require_secure_transport`。该账号的明文登录失败。密码正确的 TLS 登录成功。没有这条子句的账号仍然可以用明文连接，包括连到同时提供 TLS 的监听器。

rusql 在同时传入 `--ssl-cert` 与 `--ssl-key` 时已经出示证书（M181，[adr-m181-tls.md](adr-m181-tls.md)）。该监听器仍然接受明文。本 ADR 是账号级标志。不修改 `CONSTITUTION.md`。`REQUIRE X509`、颁发者、主题和密码套件列表不属于本切片。

## 决策

1. **标志写在账号行上。** `CREATE USER … REQUIRE SSL` 与 `ALTER USER … REQUIRE SSL` 在 `{data_dir}/mysql.user.json` 的账号上设置 `require_ssl`。`REQUIRE NONE` 清除它。该子句可以写在 `IDENTIFIED` 与 M183 的 `PASSWORD` 子句之前或之后。旧目录加载时该标志为 false。修改该子句属于管理操作（非超级用户为 errno **1227**），与 `IDENTIFIED BY` 写在一起时也一样。

2. **明文登录是 errno 1045。** 匹配到的账号带有 `require_ssl` 且套接字不是 TLS 时，服务器发送 errno **1045**（`ER_ACCESS_DENIED_ERROR`）和现有的拒绝访问文案（i18n），并且不完成认证。这是 MySQL 对账号 SSL 限制返回给客户端的 errno。errno **3159**（`ER_SECURE_TRANSPORT_REQUIRED`）仍然只用于 M181 的 `--require-secure-transport`，这里不复用。明文会话上对 `REQUIRE SSL` 账号执行 `COM_CHANGE_USER` 同样是 errno **1045**；原来的会话用户保持不变。

3. **TLS 登录成功。** 在 M181 的 rustls 监听器上，同一账号在 TLS 升级之后走现有的密码握手，并可以 `COM_QUERY`。服务器仍然不请求客户端证书。没有 `--ssl-cert` / `--ssl-key` 的监听器无法满足该标志，因此在配置 TLS 之前该账号不能登录。

4. **其他账号仍可明文。** 没有该标志的账号仍然可以不经 TLS 登录，包括监听器已有证书、以及另一个账号要求 SSL 的时候。`--require-secure-transport` 不变。

5. **`SHOW CREATE USER`。** 标志打开时，重建的语句以 ` REQUIRE SSL` 结尾。密码哈希不出现在单元格里。标志关闭时，单元格不附加 `REQUIRE SSL`（该桩仍然省略 MySQL 的其他默认子句）。

6. **跳过的形式是语法错误。** `REQUIRE X509`、`REQUIRE ISSUER`、`REQUIRE SUBJECT`、`REQUIRE CIPHER` 不修改账号。语句为 errno **1064**。它们不会被存下来，也不会被当成 `REQUIRE SSL`。

## 后果

- 需要 `REQUIRE SSL` 的运维必须用 `--ssl-cert` 和 `--ssl-key` 启动服务器。该账号的明文客户端看到 errno 1045。
- `mysql-diff` 保持 `--ssl-mode=DISABLED`。它检查 DDL 成功。明文与 TLS 的登录检查是 `cargo test -p rusql-server require_ssl`，证书在测试里临时生成，不提交进仓库。
- rusql 没有 Unix socket 监听，因此套接字不会被当成安全传输。

## 不在范围内

- `REQUIRE X509`、`REQUIRE ISSUER`、`REQUIRE SUBJECT`、`REQUIRE CIPHER`
- 请求客户端证书、`--ssl-ca` 或主题匹配
- 不重启就重新加载服务器证书（M181）

## 如何验证

```bash
cargo test -p rusql-server require_ssl
cargo test -p rusql-executor show_create_user
cargo test -p rusql-core require_ssl
```

手动验证（不要提交私钥）：

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data \
  --ssl-cert ./server.crt --ssl-key ./server.key
```

```sql
CREATE USER 'app'@'%' IDENTIFIED BY 'secret' REQUIRE SSL;
SHOW CREATE USER 'app'@'%';
```

`mysql --ssl-mode=DISABLED -u app -psecret` 为 errno **1045**。`mysql --ssl-mode=REQUIRED -u app -psecret` 可以连接。创建时没有 `REQUIRE SSL` 的用户仍然可以用 `--ssl-mode=DISABLED` 连接。

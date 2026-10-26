# ADR：M181 TLS 监听（`--ssl-cert` / `--ssl-key`）

**状态**：已接受（M181 / [#333](https://github.com/tanbamboo/rusql/issues/333)）  
**日期**：2026-10-26

## 背景

MySQL 8.0 客户端用 TLS 加密登录和命令阶段。官方客户端 `--ssl-mode=REQUIRED` 只在服务器握手通告 `CLIENT_SSL`（`0x0800`）并且随后的 TLS 握手成功时才会连接。`REQUIRED` 只加密会话，不校验 CA。`VERIFY_CA` 与 `VERIFY_IDENTITY` 才会校验。

rusql 的握手原先不带 `CLIENT_SSL`。只解析、却忽略的开关无法满足 `REQUIRED`。本 ADR 是 M181 的 TLS 监听子集。账号级 `REQUIRE SSL` / `REQUIRE X509` 属于 M184。副本 SSL 选项仍然忽略（M179）。

信任边界：本文是 M181 已接受的 TLS 决定。不修改 `CONSTITUTION.md`。阶段 X 的后续议题（角色、密码过期、审计、`mysql.user` 形状）仍各自保留 `needs-human`。

## 决策

1. **协议栈。** [rustls](https://github.com/rustls/rustls) 0.23，`ring` 提供者，加上 `tokio-rustls`。TLS 1.2 与 TLS 1.3。不发明新密码算法，不用 OpenSSL，也不自写证书校验器。服务器不请求客户端证书。

2. **命令行。** `--ssl-cert` 是 PEM 证书链。`--ssl-key` 是 PEM 私钥（PKCS#8、PKCS#1 或 SEC1）。必须成对出现。都不出现时监听保持明文。只给其中一个、路径读不到、或 PEM 被 rustls 拒绝，都是启动错误（i18n）。进程不会开始监听。私钥字节不写日志，也不提交进仓库。

3. **握手通告。** 只有成功加载证书时才设置 `CLIENT_SSL`。默认服务器，包括 `mysql-diff`（`--ssl-mode=DISABLED`），不通告该能力。

4. **升级。** 问候包（序号 0）之后，带 `CLIENT_SSL` 与 `CLIENT_PROTOCOL_41` 的 32 字节 SSL 请求（序号 1）在同一条 TCP 连接上开始 rustls 握手。服务器出示配置的证书链。MySQL 登录包是序号 2。认证包从序号 3 开始。更长的首包是普通登录，不是 TLS。

5. **明文仍然可用。** `--require-secure-transport` 关闭时，包括已启用 TLS 的监听器，不发送 SSL 请求的客户端仍走现有握手。默认本地测试保持明文。

6. **没有证书、客户端却要求 TLS。** 问候包的 `CLIENT_SSL` 位为 0。`mysql --ssl-mode=REQUIRED` 在客户端以 `CR_SSL_CONNECTION_ERROR`（2026）失败，不会登录。rusql 不为这次客户端本地拒绝再造一个服务器包。若原始客户端仍然发送 SSL 请求，服务器在序号 2 返回 errno **1043**（`ER_HANDSHAKE_ERROR`，i18n「握手错误」）。该包不会被当成用户名。

7. **`--require-secure-transport`。** 默认关闭，与 MySQL 8.0 一致。打开时，明文登录包在认证之前被拒绝，errno **3159**（`ER_SECURE_TRANSPORT_REQUIRED`）。文案为 MySQL 原文（i18n）。同一监听器上的 TLS 登录仍然成功。这是进程级开关，不是 `CREATE USER … REQUIRE SSL`（M184），本切片也不做 `SET GLOBAL`。rusql 没有 Unix socket 监听，因此没有套接字豁免。

8. **损坏的 TLS。** 合法 SSL 请求之后的字节如果不是成功的 TLS 握手，连接关闭。客户端看到 2026。服务器日志是 i18n 的 TLS 失败文案加上 rustls 错误。日志不含 PEM 内容。

9. **认证保持开启。** 密码校验、`caching_sha2_password` 与 `mysql_native_password` 不变。TLS 不会跳过认证。既有的 `CLIENT_SSL` 分支（仅在客户端设置该能力之后才接受 full-auth 明文口令）仍然适用，并且只在套接字已经是 TLS 时生效。

## 后果

- 证书在仓库外生成（`openssl` 或其他 CA）。自签名证书足以通过 `--ssl-mode=REQUIRED`。
- `mysql-diff` 继续使用 `--ssl-mode=DISABLED`，不需要证书。
- 复制数据目录不会带走 TLS 材料。证书是进程参数，不是目录文件。

## 不在范围内

- 账号级 `REQUIRE SSL`、`REQUIRE X509`、颁发者/主题检查、`--ssl-ca` 客户端认证（M184）
- 不重启即重新加载证书
- 副本客户端 TLS（`MASTER_SSL` 仍被忽略，M179）
- 企业审计 / keyring 插件（M185、M188）

## 如何验证

```bash
cargo test -p rusql-server tls_listen
cargo test -p rusql-protocol ssl_request
```

手动客户端（证书文件不得提交）：

```bash
cargo run -p rusql-server -- --port 3307 --data-dir ./rusql-data \
  --ssl-cert ./server.crt --ssl-key ./server.key
mysql --protocol=TCP -h 127.0.0.1 -P 3307 -u root --ssl-mode=REQUIRED -e "SELECT 1"
```

对未提供 `--ssl-cert` / `--ssl-key` 的服务器执行同一条 `mysql` 命令必须失败。除非设置了 `--require-secure-transport`，否则 `--ssl-mode=DISABLED` 对两种服务器都应成功；设置之后明文登录为 errno 3159。

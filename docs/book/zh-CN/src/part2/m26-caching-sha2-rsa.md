# M26 — caching_sha2 RSA 完整认证

**Issue #49**

## 问题

M7 仅实现 **fast-path**（32 字节 SHA256 scramble）。非 TLS 且无法走 fast auth 的客户端需要 MySQL 8 定义的 **RSA 公钥交换**。

## 设计

- Fast auth 成功：`AuthMoreData(0x01, 0x03)` 后 OK。
- 初始响应为空/非 fast：`AuthMoreData(0x01, 0x04)` → 客户端 `0x02` → 服务端 PEM → RSA-OAEP(SHA1) 加密 XOR 扰乱后的密码。
- 启用 `--auth-password` 时生成 2048 位 RSA 密钥对。
- `mysql_native_password` 单轮路径不变。

## Harness 经验

> `accepts_caching_sha2_rsa_when_auth_enabled` 模拟完整线缆交换（不走 fast scramble）。

## 参考

- [MySQL caching_sha2 博客（RSA 步骤）](https://dev.mysql.com/blog-archive/preparing-your-community-connector-for-mysql-8-part-2-sha256/)

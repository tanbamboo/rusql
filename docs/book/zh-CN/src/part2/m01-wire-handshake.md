# M1 — 线缆协议握手

**合并**：PR #6

## 问题

MySQL 客户端无法与不能完成**协议版本 10 握手**与能力协商的服务器通信。

## 设计选择

- `rusql-server` 中 **Tokio** TCP 监听
- 含版本串与字符集的初始握手包
- `rusql-protocol` 中 OK/ERR 包帧
- 早期公布认证插件名（M6/M7 演进）

## 取舍

实现足以连接并发送 `COM_QUERY` 的握手，而非完整 SSL、压缩或连接属性。

## 延后

真实密码校验（M6）、`caching_sha2_password`（M7）、预编译语句（M11）。

## Harness 启示

> M1 即在 `test_support` 投入**最小测试客户端**的线缆集成测试 —— 后续里程碑持续受益。

## 延伸阅读

- [adr-auth-mvp.md](../../../en/specs/adr-auth-mvp.md)

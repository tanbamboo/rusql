# M7 — caching_sha2_password

**合并**：PR #18 · 关闭 #7

## 问题

MySQL 8 默认 **`caching_sha2_password`**。未指定 `--default-auth=mysql_native_password` 的客户端无法连接 rusql。

## 设计选择

- 握手中公布 `caching_sha2_password`
- **快路径** SHA256 校验（[adr-m7](../../../en/specs/adr-m7-caching-sha2.md)）
- 保留 `mysql_native_password` 回退

## 取舍

**RSA 完整交换延后** —— 写明负向约束；满足本地开发与 CI 客户端。

## Harness 启示

> 显式写**负向约束**（「尚无 RSA」），避免单次 PR 把认证做满。

## 延伸阅读

- [adr-m7-caching-sha2.md](../../../en/specs/adr-m7-caching-sha2.md)

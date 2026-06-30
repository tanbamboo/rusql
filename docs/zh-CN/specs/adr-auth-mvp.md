# ADR：MVP 认证插件

**状态**：已接受（[#3](https://github.com/tanbamboo/rusql/issues/3#issuecomment-4840854335)）  
**日期**：2026-06-30

## 决策

**M1 阶段**：仅支持 `mysql_native_password`（选项 A）。

**后续**：握手稳定后，在独立 Issue 中增加 `caching_sha2_password`。

英文 canonical：[docs/en/specs/adr-auth-mvp.md](../en/specs/adr-auth-mvp.md)

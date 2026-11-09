# ADR：M201 UDF `.so` — 显式安全跳过

**状态**：已接受（M201 / [#354](https://github.com/tanbamboo/rusql/issues/354)）  
**日期**：2026-11-09

## 背景

MySQL 8.0 社区版支持用 `CREATE FUNCTION … SONAME 'lib.so'`（以及 `CREATE AGGREGATE FUNCTION … SONAME`）安装原生用户定义函数。该路径会 `dlopen` SQL 客户端指定的共享库。把任意 `.so` / `.dll` 加载进服务器进程是信任边界。

rusql 已实现 SQL 存储函数（`CREATE FUNCTION … RETURNS … BEGIN … END`，M63）。那不是原生 UDF。静默忽略 `SONAME`，或假装已加载而返回 OK，都会把安全缺口藏起来。`needs-human` 只用来确认是 **跳过** 还是做受限加载器。本 ADR 就是那次确认。它不修改 `CONSTITUTION.md`，也不实现插件 / 组件加载器（M202）。

## 决定

1. **原生 UDF 共享库为范围外（安全）。** rusql 在默认构建中**不** `dlopen` 用户 `.so` / `.dll` UDF。相同表述写在 [rusql-vs-mysql.md](../reports/rusql-vs-mysql.md)、英文报告和 [SKIPS.md](../../../tests/mysql-test/SKIPS.md)。

2. **`CREATE FUNCTION … SONAME` 与 `CREATE AGGREGATE FUNCTION … SONAME` 必须报错。** 这是文档化的解析拒绝（errno **1064**），i18n 消息写明原生 UDF 共享库为范围外且不会加载。不是静默忽略，也不是 OK 包。

3. **SQL 存储函数仍在范围内。** M63 的 `CREATE FUNCTION … BEGIN … END` / `DROP FUNCTION` / 标量调用不变。本次跳过不删除它们。

4. **不是插件加载器。** `INSTALL PLUGIN` / `UNINSTALL PLUGIN` / 组件加载器仍属 M202。本 ADR 不宣称 M202 已完成。

5. **不是终极目标。** 记录本次跳过只完成 M201。阶段 Z 的 M202–M210（含 M209/M210）仍未完成。HANDOFF 不得声称 MySQL 8.0 兼容已完成。

## 后果

- 安装原生 UDF 的 mysql-test 套件继续跳过，理由为本安全决定。
- 本 ADR 进入 `main` 后，Agent 清除 #354 的 `needs-human`。
- 后续里程碑不得借 M201 把 SQL 存储函数标成范围外。
- M209 可将原生 UDF `.so` 计为 **Out of scope**（安全）。其他缺失的社区行为在落地或自有 ADR 跳过前仍为 Missing。

## 本 ADR 范围外

- 受限 / 签名的 UDF 加载器
- `.so` / 组件插件加载器（M202）
- 改变存储函数语义（M63）
- 宣称 MySQL 8.0 兼容已完成

## 如何验证

```bash
cargo test -p rusql-sql udf_soname_rejected
```

```sql
CREATE FUNCTION myfunc RETURNS INTEGER SONAME 'udf_example.so';
-- 期望 errno 1064（解析拒绝）；不会加载该库。
```

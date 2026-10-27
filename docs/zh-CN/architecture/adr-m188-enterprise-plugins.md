# ADR：M188 企业插件 — 显式跳过

**状态**：已接受（M188 / [#340](https://github.com/tanbamboo/rusql/issues/340)）  
**日期**：2026-10-27

## 背景

rusql 的 MySQL 8.0 目标是社区版服务器表面。Oracle 另外提供闭源企业版服务器插件。把缺失插件当成成功，或在文档里不点名，都会把缺口藏起来。本议题就是跳过清单：写出插件名，标明 **out of scope**（范围外），并且不宣称已经加载。

路线图已经把闭源企业插件排除在完成定义之外，前提是排除必须写明。`needs-human` 只用来确认这份清单。本 ADR 就是那次确认。它不修改 `CONSTITUTION.md`，也不添加插件加载器（M202）。

rusql 的 `--audit-log`（M185）是 JSON 行文件，不是企业版审计插件。

## 决定

1. **下列 Oracle 企业版插件和组件为范围外。** 它们不属于 MySQL 8.0 社区版。rusql 不加载它们，不把它们列为已激活，也不为安装它们返回 OK。下表是 M188 的封闭清单。不在表中的名字不由本 ADR 跳过。

   | 插件 / 组件 | 产品 |
   |-------------|------|
   | `audit_log` | MySQL Enterprise Audit（企业版审计） |
   | `mysql_firewall` | MySQL Enterprise Firewall（企业版防火墙） |
   | `data_masking` | MySQL Enterprise Data Masking and De-identification（数据脱敏） |
   | `component_masking` | 数据脱敏的组件形式（8.0.33+），包含 `component_masking_functions` |
   | `thread_pool` | MySQL Enterprise Thread Pool 插件（线程池插件） |
   | `authentication_pam` | 企业版 PAM 认证 |
   | `authentication_ldap_sasl` | 企业版 LDAP（SASL） |
   | `authentication_ldap_simple` | 企业版 LDAP（简单绑定） |
   | `authentication_windows` | 企业版 Windows 本机认证 |
   | `keyring_okv` | Oracle Key Vault 密钥环 |
   | `keyring_aws` | AWS 密钥环 |
   | `keyring_hashicorp` | HashiCorp Vault 密钥环 |
   | `keyring_oci` | OCI Vault 密钥环 |
   | `enterprise_encryption` | MySQL Enterprise Encryption（企业版加密） |
   | `version_tokens` | 企业版 version tokens |

   同一组标识符写在 [rusql-vs-mysql.md](../reports/rusql-vs-mysql.md)、英文报告和 [SKIPS.md](../../../tests/mysql-test/SKIPS.md)。这些文档里要能找到：audit、firewall、data masking、thread pool plugin。

2. **阶段 W–Y 仍需要的社区插件保持在范围内。** 这次跳过不包括：

   - `performance_schema`（阶段 Y，M189）
   - 慢查询日志、通用查询日志和 `--log-error`（M191、M192、M195）
   - `validate_password`（社区功能；M183 内建，不是 `.so`）
   - `caching_sha2_password` 与 `mysql_native_password`
   - `rpl_semi_sync_master`（社区半同步，M178）
   - `keyring_file`（社区密钥环；`keyring_encrypted_file` 也是社区版）
   - `rewriter` 与 `connection_control`（社区插件）
   - `authentication_kerberos` 与 `authentication_fido`（8.0 社区版）
   - `group_replication`（M199）和 `clone`（M200）：阶段 Z 的社区工作，不是企业插件
   - UDF `.so`（M201）以及 `INSTALL PLUGIN` / 组件加载器（M202）

3. **没有加载器，也没有成功包。** 本变更不实现 `INSTALL PLUGIN`、`UNINSTALL PLUGIN` 或 `INSTALL COMPONENT`，也不添加“这些插件已加载”的记录。加载器属于 M202。禁止伪造 OK。

4. **不是北极星目标。** 写明这次跳过即完成 M188，也是阶段 X 的最后一项。阶段 Y–Z（含 M209、M210）仍然需要。HANDOFF 不得写成 MySQL 8.0 目标已完成。

MySQL Enterprise Backup 与 MySQL Enterprise Monitor 是商业产品，不是上表中的服务器插件。rusql 不提供它们。它们不进入插件表，以免和社区插件混淆。

## 后果

- 安装或断言上表插件的 mysql-test 文件继续跳过，理由就是这份清单。
- M185 的 `--audit-log` 仍然可用，并且仍然不是 `audit_log`。
- 后续里程碑不得引用 M188，把 `performance_schema`、半同步、`validate_password`、`group_replication` 或 `clone` 标成范围外。
- M209 只能把上表计为 **out of scope**。其余缺失的社区行为在落地或另有 ADR 跳过之前仍是 Missing。

## 本 ADR 不做

- `.so` / 组件加载器（M202）
- 把社区 UDF `.so` 标成范围外（M201）
- 实现防火墙规则、脱敏函数或线程池调度
- 修改 `CONSTITUTION.md`

## 如何验证

```bash
node scripts/doc-parity.mjs
```

上表每个插件标识符都出现在 `docs/en/reports/rusql-vs-mysql.md`、`docs/zh-CN/reports/rusql-vs-mysql.md` 和 `tests/mysql-test/SKIPS.md`。没有 crate 宣称这些插件已加载。

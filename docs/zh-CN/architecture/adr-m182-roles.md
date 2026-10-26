# ADR：M182 角色（`CREATE ROLE` / `GRANT role` / `SET ROLE`）

**状态**：已接受（M182 / [#334](https://github.com/tanbamboo/rusql/issues/334)）  
**日期**：2026-10-26

## 背景

MySQL 8.0 把角色当作一组命名权限。`CREATE ROLE` 定义名字。`GRANT SELECT ON db.* TO role` 把权限挂到这个名字上。`GRANT role TO user` 记录一条边。在会话执行 `SET ROLE` 之前，这些权限不生效。`CURRENT_ROLE()` 报告当前激活的名字。`REVOKE role FROM user` 删掉这条边，下一条语句不再拥有这些权限。

rusql 已经把直接的 `GRANT` / `REVOKE` 行存在 `{data_dir}/mysql.user.json`（M54），登录账号也在同一个文件里（M55）。本 ADR 是 M182 的角色图。不修改 `CONSTITUTION.md`。强制角色、密码过期、审计，以及 `mysql.user` 表形态，仍留在各自的议题。

## 决策

1. **目录。** 角色是 `mysql.user.json` 里的行，和登录账号分开。角色不能认证。省略主机时为 `%`。`CREATE ROLE` 遇到已存在的角色或登录账号是 errno **1396**。`DROP ROLE` 遇到未知角色是 errno **1396**。`IF NOT EXISTS` / `IF EXISTS` 不再报这个错误。删除角色时，同时删除它的权限行以及所有提到它的边。

2. **边。** `GRANT role [, role] TO user_or_role [, user_or_role]` 与 `REVOKE role FROM user_or_role` 把边写进同一个文件。角色必须已存在（否则 errno **3523**）。被授予者必须是已有账号、已有角色，或 `root`（否则 errno **3523**）。空用户名是 errno **3876**。重复授予已有边会成功。撤销一条不存在的边是 errno **3530**。

3. **环。** MySQL 允许环（`GRANT r1 TO r2` 与 `GRANT r2 TO r1`，也包括自己授予自己）。rusql 保存这条边，并且**检测**环。权限查找用已访问集合遍历，遇到重复节点就停止，因此环不会造成死循环。环不会带来无环闭包之外的权限。

4. **激活。** `SET ROLE` 是会话状态，不写入 `mysql.user.json`。`COM_RESET_CONNECTION` / `COM_CHANGE_USER` 会清空它。形式：一个或多个角色名、`NONE`、`ALL`、`ALL EXCEPT role`，以及 `DEFAULT`。具名角色必须**直接**授予当前账号，否则 errno **3530**，激活集合不变。`ALL` 激活全部直接授予。`DEFAULT` 什么都不激活：本切片不保存默认角色，也不实现强制角色或 `activate_all_roles_on_login`。

5. **执行检查。** 语句检查使用直接权限，加上每个仍然被授予的活动角色的权限，包括授予这些角色的角色。`SET ROLE` 之前，角色授予不允许语句执行（errno **1142**）。`REVOKE` 或 `DROP ROLE` 之后，下一条语句会从激活集合去掉该角色并拒绝语句。`root` 在权限检查上仍是超级用户。`SET ROLE` 不会为 `root` 跳过「必须已被授予」这条规则。没有 `FROM` 的 `SELECT`（包括 `CURRENT_ROLE()`）不要求表权限。

6. **`CURRENT_ROLE()`。** 没有活动角色时返回 MySQL 标记 `NONE`。否则值是用逗号连接的 `` `name`@`host` ``，顺序与激活顺序一致。`SET ROLE ALL` 按名字再按主机排序。

7. **`SHOW GRANTS`。** 权限行保持 M54 的形状。每条直接角色边增加 `` GRANT `role`@`host` TO 'user'@'host' ``。行会排序。这是文档中的格式。不是 `SHOW GRANTS ... USING` 展开，也不是 MySQL 在账号上使用的反引号。

8. **谁可以改目录。** `CREATE ROLE` / `DROP ROLE` 使用与 `CREATE USER` 相同的超级用户门槛（errno **1227**，i18n 文案写的是 `CREATE ROLE`）。角色的 `GRANT` / `REVOKE` 使用现有权限授予的超级用户门槛（errno **1142**）。没有单独的 `CREATE ROLE` 权限位，也没有 `WITH ADMIN OPTION`。

9. **旧文件。** 缺少 `roles` 与 `role_edges` 时默认为空，因此 M182 之前写下的数据目录仍然能加载。

## 后果

- 复制数据目录会把角色图和其他权限行一起复制走。
- 登录账号和角色不能共用同一个 `user`@`host`。
- 把登录账号当作角色来授予会被 errno **3523** 拒绝。只有 `CREATE ROLE` 创建的名字才是角色。

## 不在范围内

- 强制角色与 `activate_all_roles_on_login`
- `SET DEFAULT ROLE`（因此 `SET ROLE DEFAULT` 为空）
- 角色授予上的 `WITH ADMIN OPTION`
- `SHOW GRANTS ... USING`
- 企业审计（M185）

## 如何验证

```bash
cargo test -p rusql-sql create_role
cargo test -p rusql-executor roles
cargo test -p rusql-server roles_grant_set_role_allows_select
```

手工：

```sql
CREATE ROLE r;
GRANT SELECT ON rusql.* TO r;
CREATE USER 'app'@'%' IDENTIFIED BY 'secret';
GRANT r TO 'app'@'%';
-- 以 app 连接
SELECT CURRENT_ROLE();          -- NONE
SET ROLE r;
SELECT CURRENT_ROLE();          -- `r`@`%`
```

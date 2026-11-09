# ADR：M199 Group Replication 成员子集

**状态**：已接受（M199 / [#352](https://github.com/tanbamboo/rusql/issues/352)）  
**日期**：2026-11-09

## 背景

MySQL 8.0 的 Group Replication（以及其上的 InnoDB Cluster）通过 GCS/XCom 成员层、认证与冲突处理提供多主或单主高可用。rusql 已有异步 binlog dump/apply（M179）、手动提升（M177）以及持久化的 `@@gtid_executed`（M176）。从零实现 XCom 超出本里程碑，也无法成为忠实的 Oracle 线协议对端。若 GR 未标记，M210 不能关闭。禁止静默跳过。本决定不修改 `CONSTITUTION.md`。

## 决定

1. **实现 Group Replication 成员子集**，不是仅推迟到 M177，也不是静默跳过。社区版 `group_replication` 仍在范围内；本切片是第一个可查询的成员面。

2. **不从零实现 XCom。** 没有 GCS 协议、没有组通信消息，也不声称与 Oracle Group Replication 线协议兼容。

3. **配置驱动的三节点成员。** 在 `{data_dir}/group_replication.json` 中持久化组名、本地成员 id，以及三个成员端点（或在首次 `START` 时种入三个进程内逻辑成员）。本切片不通过网络发现成员。

4. **SQL 控制。** `START GROUP_REPLICATION` 在单主模式下将三个成员标为 `ONLINE`（本地成员为 `PRIMARY`，另外两个为 `SECONDARY`）并返回 OK。`STOP GROUP_REPLICATION` 清除运行中的成员（零行）。已在运行是 errno **3093**。无法形成三个成员的目录是 errno **3092**。

5. **`performance_schema.replication_group_members` 桩。** 列：`MEMBER_ID`、`MEMBER_HOST`、`MEMBER_PORT`、`MEMBER_STATE`、`MEMBER_ROLE`、`MEMBER_VERSION`。启动后 `SELECT` 返回三行；停止后为零行。其他 GR 的 P_S 表仍为 errno **1146**。

6. **不是完整的 InnoDB Cluster**，也不是自动故障转移、冲突认证或分布式恢复。Clone 仍属 M200。

## 后果

- 本 ADR 进入 `main` 后，可为 #352 清除 `needs-human`。
- 测试证明 `START GROUP_REPLICATION` 之后有三个成员。
- 兼容性文档写明这是成员桩，不是 InnoDB Cluster / Oracle GR。
- M210 仍需要后续 Phase Z 工作；接受本 ADR 并不完成 MySQL 8.0。

## 不在范围

- 从零实现 XCom / GCS
- 多主模式与冲突认证
- Oracle Group Replication 线协议
- InnoDB Cluster 元数据 / MySQL Shell 管理 API
- Clone 插件（M200）
- 声称已完成 MySQL 8.0 兼容

## 如何验证

```bash
cargo test -p rusql-storage group_replication
cargo test -p rusql-executor group_replication
cargo test -p rusql-server group_replication
```

```sql
START GROUP_REPLICATION;
SELECT MEMBER_ID, MEMBER_HOST, MEMBER_PORT, MEMBER_STATE, MEMBER_ROLE
  FROM performance_schema.replication_group_members;
STOP GROUP_REPLICATION;
```

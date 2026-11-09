# ADR：M200 Clone / 数据目录快照

**状态**：已接受（M200 / [#353](https://github.com/tanbamboo/rusql/issues/353)）  
**日期**：2026-11-09

## 背景

MySQL 8.0 社区版提供 `clone` 插件，支持 `CLONE INSTANCE`（远程 donor）与 `CLONE LOCAL DATA DIRECTORY`，用于配制备份。rusql 已有异步 binlog apply（M179）与崩溃安全的 WAL 重放（M198）。把 Oracle 插件 `.so` 装进 `mysql.plugin` 不在范围内（M188 禁止假装企业插件已加载；社区版 `clone` 仍在范围内，但不伪造插件行）。禁止静默跳过。验收标准默认优先实现 rusql 的 clone/快照命令，而不是仅文档替代。本决定不修改 `CONSTITUTION.md`。

## 决定

1. **实现文件级数据目录快照**，不是已加载的 clone 插件，也不是 `mysql.plugin` 中的一行。

2. **SQL 子集。** `CLONE LOCAL DATA DIRECTORY [=] 'path'`（以及本地别名 `CLONE INSTANCE DATA DIRECTORY [=] 'path'`）把服务器当前 `--data-dir` 复制到空目标目录（或创建该目录）。适用于为副本播种数据目录。远程 `CLONE INSTANCE FROM 'user'@'host':port …` 以明确的解析错误拒绝（errno **1064**）。

3. **机制。** 复制前，若存在打开的 WAL 则先 `fsync`。仅递归复制源数据目录下的普通文件与目录。不跟随指向数据目录外的符号链接。不从数据目录外复制额外机密文件。目标必须为空或不存在；非空为 errno **3869**。I/O 失败为 errno **3865**。

4. **不加全局锁。** Clone 不获取全局读锁。并发写入可能导致撕裂快照。需要一致的副本种子时，请先静默写入方或停止 donor。用户指南中说明这一点。

5. **不是** Oracle clone 插件线协议，不是自动 Group Replication 分布式恢复，也不是 M201/M202。

## 后果

- 本 ADR 进入 `main` 后，可为 #353 清除 `needs-human`。
- 存储夹具证明 `clone_instance` 会复制 WAL，并在目标上重放已提交行。
- 兼容性文档写明这是本地数据目录快照，不是已加载插件。
- M210 仍需要后续 Phase Z 工作；接受本 ADR 并不完成 MySQL 8.0。

## 不在范围

- 将 `clone` 装入 `mysql.plugin`
- 远程 donor `CLONE INSTANCE FROM`
- Clone 插件网络协议与进度表
- 通过 clone 做自动 GR 分布式恢复
- 同一次变更中的 M201 UDF / M202 `INSTALL PLUGIN`
- 声称已完成 MySQL 8.0 兼容

## 如何验证

```bash
cargo test -p rusql-storage clone_instance
cargo test -p rusql-executor clone
cargo test -p rusql-server clone_instance
```

```sql
CLONE LOCAL DATA DIRECTORY = '/path/to/empty/or/new/dir';
-- 然后用 --data-dir 指向该路径再启动另一个 rusql。
```

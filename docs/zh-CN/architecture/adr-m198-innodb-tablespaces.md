# ADR：M198 与 InnoDB 表空间等价的崩溃恢复

**状态**：已接受（M198 / [#351](https://github.com/tanbamboo/rusql/issues/351)）  
**日期**：2026-10-29

## 背景

MySQL 8.0 的 InnoDB 用重做日志、吸收撕裂页的双写缓冲，以及表空间文件（`innodb_file_per_table` 默认为 `ON` 时是 `.ibd`）做崩溃恢复。rusql 已经把已提交的变更写进追加式 JSON 行 WAL（`{data_dir}/rusql.wal`），表堆在内存里。重启时重放这份 WAL。把“堆 + WAL”说成 InnoDB 表空间是假话。本议题要求做对照、用测试证明提交之后被杀掉的数据在重新打开后还在，并规定没有这份 ADR 就不能关闭 M210。本决定不修改 `CONSTITUTION.md`。

## 决定

1. **保留 rusql WAL。** 不增加 `.ibd`、`ibdata` 系统表空间或 InnoDB 重做文件。不丢掉 WAL。`SHOW ENGINES` 不变：`InnoDB` 仍是既有的兼容名字，不是表空间实现。

2. **客户端能看到的持久性**（默认 `--wal-sync always`，即 `WalSyncPolicy::Always`）：
   - `COMMIT` 和自动提交语句在调用返回前把 JSON 行追加进 WAL 并 `fsync`。
   - 在那次返回之后杀掉进程会丢掉内存堆。用同一个数据目录重新打开会重放 WAL。那些已提交的行还在。
   - 还没到 `COMMIT` 的事务不在。
   - 末尾没有换行的半行是撕裂写入。恢复时丢掉它，把文件截断到最后一条完整的行，再同步。更早的完整行保留，下一次追加是新的一行。
   - 一条完整的行无法解码就是损坏。恢复失败，并且不截断文件。

3. **对照。** 这是等价的保证，不是文件格式兼容。

   | InnoDB 8.0 | rusql |
   |------------|--------|
   | 重做日志 | `{data_dir}/rusql.wal` 的 JSON 行，打开时重放 |
   | 双写缓冲（表空间里的撕裂页） | 没有页，也没有双写缓冲。追加日志末尾的撕裂部分会被丢掉 |
   | `.ibd` 与 `innodb_file_per_table=ON` | 未实现。所有表共用一份 WAL。`SELECT @@innodb_file_per_table` 是 errno **1193**（未知系统变量）。这也不是 InnoDB 的 `OFF`（`ibdata1`） |
   | `innodb_flush_log_at_trx_commit=1` | 默认 `always`：自动提交和 `COMMIT` 都 `fsync` |
   | 更弱的刷盘设置 | `--wal-sync batch` 只同步显式 `COMMIT`，不按语句同步自动提交。`--wal-sync none` 不 `fsync`。两者都不是“提交之后被杀掉仍然在”的保证 |
   | 重做提交记录加 undo | 普通 `COMMIT` 是一批行，然后一次 `fsync`。在追加过程中、`fsync` 返回之前被杀掉，如果操作系统留下了那些行，恢复可能只重放这批里的前缀。`COMMIT` 返回之后，`fsync` 已经完成 |
   | 打开 MySQL 的 `.ibd` | 不宣称，也不支持 |

4. **M210。** [#363](https://github.com/tanbamboo/rusql/issues/363) 在这份 ADR 被 `main` 接受之前不能标完成。接受它并不完成 M210、M209，也不完成 MySQL 8.0 北极星目标。上面的缺口仍留在兼容报告里，给那份矩阵用。

## 后果

- `cargo test -p rusql-storage crash_recovery` 会提交一行，丢掉引擎（杀掉进程的模型：堆不在磁盘上，`PersistentEngine` 没有 `Drop` 刷盘），再追加一段撕裂尾部，然后检查已提交的行还在、撕裂字节已经消失。
- 用同一个 `--data-dir` 重新启动的服务器能看到这些行。它不会创建 `.ibd` 文件。
- `information_schema` 里的表空间名字仍是 errno **1146**（M194）。

## 范围外

- `.ibd`、`ibdata1` 和 InnoDB 重做文件兼容
- 双写缓冲
- 会创建文件的 `innodb_file_per_table` 开关
- `information_schema.INNODB_TABLESPACES`
- 宣称 MySQL 8.0 兼容已经完成

## 如何验证

```bash
cargo test -p rusql-storage crash_recovery
```

```sql
CREATE TABLE t (id INT PRIMARY KEY);
INSERT INTO t VALUES (1);
-- 这条语句返回后停掉进程，再用同一个 --data-dir 启动。
SELECT id FROM t;
```

`SELECT @@innodb_file_per_table` 是 errno **1193**。

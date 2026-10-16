# ADR：M161 间隙锁 / next-key 锁（InnoDB RR MVP）

**状态**：已接受（M161 / [#313](https://github.com/tanbamboo/rusql/issues/313)）  
**日期**：2026-10-17

## 背景

MySQL InnoDB 在 **REPEATABLE READ** 下对加锁读使用 **next-key 锁**（记录锁 + 记录前间隙锁），防止在已扫描范围内发生 **幻读插入**。rusql 已有 M157 行排他锁（`SELECT … FOR UPDATE`）与 M160 行等待死锁检测。

M161 增加最小间隙锁层：对 **范围** `FOR UPDATE`（或 SERIALIZABLE 隐式加锁）阻塞并发 **INSERT** 落在该 PK 范围内的主键。

## 术语（InnoDB vs rusql MVP）

| 概念 | InnoDB | rusql M161 |
|------|--------|------------|
| 记录锁 | 已有索引记录上的排他锁 | 已有 `RowKey` 排他锁（M157） |
| 间隙锁 | 索引记录之间开区间的锁 | `GapLockKey`：单列 PK 上有界区间 |
| next-key 锁 | 记录前间隙 + 记录 | 扫描行上的记录锁 **加上** 扫描谓词对应的一个区间间隙锁 |
| 插入意向 | 特殊锁模式 | INSERT 在冲突间隙/行锁上等待，再对新 PK 加行锁 |

## 决策

1. **范围**：仅 **单列 PRIMARY KEY** 表（与 M157 相同）。整型 PK 按数值序；其他类型按字符串序。

2. **何时加间隙锁**：`SELECT … FOR UPDATE`（含 SERIALIZABLE 隐式 `FOR UPDATE`），且隔离级别为 **REPEATABLE READ** 或 **SERIALIZABLE**，且 `WHERE` 为 **PK 范围**：
   - PK 列 `BETWEEN low AND high`，或
   - PK 列 `>= low AND <= high`（及 `>`、`<` 变体）。

   仅等值谓词仍只加 **记录锁**（不额外加区间间隙锁）。

3. **间隙锁形态**：每个范围扫描一个含端点开闭的区间 `[low, high]`，与行锁共用 `RowLockRegistry`。扫描到的行仍加记录锁。

4. **INSERT / REPLACE（新行）**：写入前若其他会话持有覆盖新 PK 的间隙锁或该行锁，则 **等待**（`innodb_lock_wait_timeout`，errno **1205**）。同一会话可重入。

5. **死锁**：M160 等待图扩展到 **间隙锁目标**；环上较新会话 errno **1213**。

6. **READ COMMITTED**：加锁读不加间隙锁（仅记录锁），与 InnoDB RC 一致。

## 后果

- 无 PK 范围谓词的全表扫描 **不会** 加全表间隙锁（M159 SERIALIZABLE 普通 `SELECT` 除存在范围谓词外仍为行锁 MVP）。
- 本切片不含 `FOR SHARE` 间隙锁、谓词锁、`SKIP LOCKED` / `NOWAIT` 间隙行为。

## 不在范围内

- 完整 InnoDB 锁兼容矩阵
- 超出显式/隐式加锁读的 SERIALIZABLE 谓词锁
- `SHOW ENGINE INNODB STATUS` 锁报告

# ADR：M197 FULLTEXT 布尔子集

**状态**：已接受（M197 / [#350](https://github.com/tanbamboo/rusql/issues/350)）  
**日期**：2026-10-29

## 背景

MySQL 8.0 的 InnoDB FULLTEXT 是一套大引擎：倒排索引、停用词、`innodb_ft_min_token_size`、ngram 与 MeCab 解析器、自然语言相关度，以及查询扩展。本议题要求在 `VARCHAR` / `TEXT` 上建倒排索引，使 `MATCH … AGAINST ('+word' IN BOOLEAN MODE)` 返回包含该词的行，并钉死没有 FULLTEXT 索引时的行为。相关度不能说成与 InnoDB 相同。查询扩展不在这一片。本决定不修改 `CONSTITUTION.md`。

## 决定

1. **不新增 crate。** 分词和倒排索引放在 `rusql-storage`。sqlparser 0.53 能解析 `CREATE INDEX` 和 `CREATE TABLE … FULLTEXT`，不能解析 `CREATE FULLTEXT INDEX`，所以 `rusql-sql` 先改写这条语句。`MATCH` 求值放在 `rusql-executor`。`IndexMeta.fulltext` 是目录标志（必须有，否则 FULLTEXT 索引会被当成 B 树等值键）。

2. **DDL。** `CREATE FULLTEXT INDEX name ON t (col[, …])`、`CREATE TABLE` 里的 `FULLTEXT` / `FULLTEXT INDEX` / `FULLTEXT KEY`，以及 `ALTER TABLE … ADD FULLTEXT`。列必须是 `CHAR`、`VARCHAR`、`TINYTEXT`、`TEXT`、`MEDIUMTEXT` 或 `LONGTEXT`。其他类型是 errno **1283**。列不存在是 errno **1072**。索引会用已经存下的行回填，之后的 `INSERT`、`UPDATE`、`DELETE` 会维护它。

3. **没有索引。** `MATCH (cols) AGAINST (…)` 若没有覆盖同一列列表（顺序相同）的 FULLTEXT 索引，是 errno **1191**（`Can't find FULLTEXT index matching the column list`）。不会改成全表扫描。

4. **布尔模式。** `IN BOOLEAN MODE` 支持 `+word`（必须出现）、`-word`（不能出现）和裸词（仅当没有 `+word` 时，出现其中任意一个即可）。一行被返回的条件是：每个必须词都在、每个排除词都不在；如果没有必须词，则至少有一个可选词。词是至少 3 个字符的 ASCII 字母或数字，并转成小写。这与 InnoDB 默认最短长度一致，因此 `foo` 能命中。这不是相关度分数。短语引号、`*`、括号、`<`、`>`、`~`、`@` 是 errno **1235**。

5. **不在这一片。** 自然语言模式（包括没有修饰符的 `MATCH … AGAINST ('word')`）在已有 FULLTEXT 索引时是 errno **1235**，没有索引时是 errno **1191**。`WITH QUERY EXPANSION` 是 errno **1235**。没有停用词表，没有 ngram 解析器，也没有查询扩展。`SPATIAL INDEX` 是 errno **1235**。

## 后果

- `CREATE FULLTEXT INDEX` 加上 `SELECT … WHERE MATCH(col) AGAINST ('+foo' IN BOOLEAN MODE)` 会返回文本里含有词元 `foo` 的行。mysql-diff 套件 `fulltext_boolean` 用这份 id 列表和 MySQL 8.0 比较，并比较没有索引、以及非文本列上的失败。
- 布尔结果只表示“包含 / 不包含”。它不是 InnoDB 的相关度浮点数。不要拿 `SELECT MATCH(…) AGAINST (…)` 的分数和 MySQL 比较。
- 重启时用 WAL 标记和已存的行重建 FULLTEXT 索引。

## 范围外

- InnoDB 相关度、自然语言 50% 阈值，以及查询扩展
- 停用词、ngram / MeCab，以及中文分词
- `SPATIAL INDEX`（M196 同样没有空间索引）
- 宣称 MySQL 8.0 兼容已经完成

## 如何验证

```bash
cargo test -p rusql-storage fulltext
cargo test -p rusql-executor match_against
cargo test -p rusql-server match_against
```

```sql
CREATE TABLE ft_t (id INT PRIMARY KEY, body VARCHAR(64));
INSERT INTO ft_t VALUES (1, 'foo bar'), (2, 'baz qux');
CREATE FULLTEXT INDEX ft_body ON ft_t (body);
SELECT id FROM ft_t WHERE MATCH(body) AGAINST ('+foo' IN BOOLEAN MODE);
```

# ADR: M197 FULLTEXT boolean subset

**Status**: Accepted (M197 / [#350](https://github.com/tanbamboo/rusql/issues/350))  
**Date**: 2026-10-29

## Context

MySQL 8.0 InnoDB FULLTEXT is a large engine: an inverted index, a stopword list, `innodb_ft_min_token_size`, ngram and MeCab parsers, natural-language ranking, and query expansion. The issue asks for an inverted index on `VARCHAR` / `TEXT` so `MATCH … AGAINST ('+word' IN BOOLEAN MODE)` returns rows that contain that word, and it asks us to pin what happens when no FULLTEXT index exists. Ranking must not be described as identical to InnoDB. Query expansion is out of this slice. This decision does not change `CONSTITUTION.md`.

## Decision

1. **No new crate.** Tokenizing and the inverted index live in `rusql-storage`. `CREATE FULLTEXT INDEX` is rewritten in `rusql-sql` because sqlparser 0.53 accepts `CREATE INDEX` and `CREATE TABLE … FULLTEXT`, not `CREATE FULLTEXT INDEX`. `MATCH` evaluation lives in `rusql-executor`. `IndexMeta.fulltext` is the catalog flag (required so a FULLTEXT index is not reused as a B-tree equality key).

2. **DDL.** `CREATE FULLTEXT INDEX name ON t (col[, …])`, `CREATE TABLE` `FULLTEXT` / `FULLTEXT INDEX` / `FULLTEXT KEY`, and `ALTER TABLE … ADD FULLTEXT`. Columns must be `CHAR`, `VARCHAR`, `TINYTEXT`, `TEXT`, `MEDIUMTEXT`, or `LONGTEXT`. Anything else is errno **1283**. A missing column is errno **1072**. The index is rebuilt from rows already stored and maintained on later `INSERT`, `UPDATE`, and `DELETE`.

3. **No index.** `MATCH (cols) AGAINST (…)` without a FULLTEXT index on that same column list (same order) is errno **1191** (`Can't find FULLTEXT index matching the column list`). It is not a table scan.

4. **Boolean mode.** `IN BOOLEAN MODE` supports `+word` (required), `-word` (excluded), and a bare word (optional, and only required when there is no `+word`). A row is returned when every required token is present, no excluded token is present, and, if there are no required tokens, at least one optional token is present. Tokens are ASCII alphanumeric runs of at least 3 characters, lowercased. That matches the InnoDB default minimum length for `foo`. It is not a score. Phrase quotes, `*`, parentheses, `<`, `>`, `~`, and `@` are errno **1235**.

5. **Not in this slice.** Natural language mode (including `MATCH … AGAINST ('word')` with no modifier) is errno **1235** when a FULLTEXT index exists, and errno **1191** when it does not. `WITH QUERY EXPANSION` is errno **1235**. There is no stopword list, no ngram parser, and no query expansion. `SPATIAL INDEX` is errno **1235**.

## Consequences

- `CREATE FULLTEXT INDEX` plus `SELECT … WHERE MATCH(col) AGAINST ('+foo' IN BOOLEAN MODE)` returns the rows whose text contains the token `foo`. mysql-diff suite `fulltext_boolean` compares that id list with MySQL 8.0, and compares the no-index and non-text-column failures.
- The boolean result is inclusion only. It is not InnoDB's relevance float. Do not compare `SELECT MATCH(…) AGAINST (…)` scores with MySQL.
- A restart replays the FULLTEXT index from the WAL marker and the stored rows.

## Out of scope

- InnoDB ranking, the 50% natural-language threshold, and query expansion
- Stopwords, ngram / MeCab, and Chinese tokenization
- `SPATIAL INDEX` (M196 does not add spatial indexes either)
- Claiming MySQL 8.0 compatibility is complete

## How to verify

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

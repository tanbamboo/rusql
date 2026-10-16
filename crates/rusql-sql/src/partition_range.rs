//! Strip and validate MySQL `PARTITION BY RANGE` on CREATE TABLE (M152).

use std::cell::RefCell;

use rusql_core::{RangePartitionBound, RangePartitionDef, RangePartitionMeta};

use crate::SqlError;

thread_local! {
    static PENDING_RANGE_PARTITION: RefCell<Option<PendingRangePartition>> =
        const { RefCell::new(None) };
}

/// Parsed RANGE partition clause waiting for executor catalog merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRangePartition {
    pub table: String,
    pub meta: RangePartitionMeta,
}

pub fn take_pending_range_partition() -> Option<PendingRangePartition> {
    PENDING_RANGE_PARTITION.with(|p| p.borrow_mut().take())
}

/// Remove MySQL RANGE partition DDL so sqlparser can parse the statement.
pub fn rewrite_create_table_range_partitions(sql: &str) -> Result<String, SqlError> {
    PENDING_RANGE_PARTITION.with(|p| *p.borrow_mut() = None);
    let trimmed = sql.trim();
    let upper = trimmed.to_ascii_uppercase();
    if !upper.starts_with("CREATE TABLE") {
        return Ok(sql.to_string());
    }
    let Some(body_start) = trimmed.find('(') else {
        return Ok(sql.to_string());
    };
    let table_part = trimmed[..body_start].trim();
    let Some(table_name) = extract_create_table_name(table_part) else {
        return Ok(sql.to_string());
    };

    let mut depth = 0i32;
    let mut body_end = None;
    for (i, ch) in trimmed.char_indices().skip(body_start) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    body_end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(body_end) = body_end else {
        return Ok(sql.to_string());
    };

    let suffix = trimmed[body_end + 1..].trim_start();
    if !suffix.to_ascii_uppercase().starts_with("PARTITION BY") {
        return Ok(sql.to_string());
    }

    let (meta, consumed) = parse_partition_by_suffix(suffix)?;
    validate_range_meta(&meta)?;
    PENDING_RANGE_PARTITION.with(|p| {
        *p.borrow_mut() = Some(PendingRangePartition {
            table: table_name.clone(),
            meta,
        });
    });
    let rest = suffix[consumed..].trim_start();
    let head = trimmed[..=body_end].trim_end();
    if rest.is_empty() {
        Ok(head.to_string())
    } else {
        Ok(format!("{head} {rest}"))
    }
}

fn validate_range_meta(meta: &RangePartitionMeta) -> Result<(), SqlError> {
    if meta.partitions.is_empty() {
        return Err(SqlError::Parse(
            rusql_i18n::messages::sql_range_partition_empty(),
        ));
    }
    let last = meta.partitions.last().expect("checked");
    if !matches!(last.less_than, RangePartitionBound::MaxValue) {
        return Err(SqlError::Parse(
            rusql_i18n::messages::sql_range_partition_requires_maxvalue(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for p in &meta.partitions {
        if !seen.insert(p.name.to_ascii_lowercase()) {
            return Err(SqlError::Parse(
                rusql_i18n::messages::sql_range_partition_duplicate_name(&p.name),
            ));
        }
    }
    Ok(())
}

fn parse_partition_by_suffix(suffix: &str) -> Result<(RangePartitionMeta, usize), SqlError> {
    let bytes = suffix.as_bytes();
    let mut i = 0usize;
    let read_word = |i: &mut usize| -> Option<String> {
        while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
        let start = *i;
        while *i < bytes.len() && (bytes[*i].is_ascii_alphanumeric() || bytes[*i] == b'_') {
            *i += 1;
        }
        if start == *i {
            None
        } else {
            Some(String::from_utf8_lossy(&bytes[start..*i]).to_string())
        }
    };
    let expect_word = |i: &mut usize, expected: &str| -> Result<(), SqlError> {
        let w = read_word(i).ok_or_else(|| parse_err("expected PARTITION BY"))?;
        if !w.eq_ignore_ascii_case(expected) {
            return Err(parse_err(&format!("expected {expected}")));
        }
        Ok(())
    };

    expect_word(&mut i, "PARTITION")?;
    expect_word(&mut i, "BY")?;
    let kind = read_word(&mut i).ok_or_else(|| parse_err("expected RANGE"))?;
    if kind.eq_ignore_ascii_case("HASH")
        || kind.eq_ignore_ascii_case("KEY")
        || kind.eq_ignore_ascii_case("LIST")
    {
        return Err(SqlError::Parse(
            rusql_i18n::messages::sql_partition_by_type_unsupported(&kind),
        ));
    }
    if !kind.eq_ignore_ascii_case("RANGE") {
        return Err(SqlError::Parse(
            rusql_i18n::messages::sql_partition_by_type_unsupported(&kind),
        ));
    }
    let key_column = parse_parenthesized_ident(suffix, &mut i)?;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'(' {
        return Err(parse_err("expected partition definition list"));
    }
    i += 1;
    let mut partitions = Vec::new();
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        expect_word(&mut i, "PARTITION")?;
        let name = read_word(&mut i).ok_or_else(|| parse_err("expected partition name"))?;
        expect_word(&mut i, "VALUES")?;
        expect_word(&mut i, "LESS")?;
        expect_word(&mut i, "THAN")?;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let less_than =
            if i + 8 <= bytes.len() && suffix[i..].to_ascii_uppercase().starts_with("MAXVALUE") {
                i += "MAXVALUE".len();
                RangePartitionBound::MaxValue
            } else {
                let bound_str = parse_parenthesized_expr(suffix, &mut i)?;
                let bound: i64 = bound_str.trim().parse().map_err(|_| {
                    SqlError::Parse(rusql_i18n::messages::sql_range_partition_bound_not_int())
                })?;
                RangePartitionBound::Value(bound)
            };
        partitions.push(RangePartitionDef { name, less_than });
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b',' {
            i += 1;
            continue;
        }
        if i < bytes.len() && bytes[i] == b')' {
            i += 1;
            break;
        }
        return Err(parse_err("expected ',' or ')' in partition list"));
    }
    Ok((
        RangePartitionMeta {
            key_column,
            partitions,
        },
        i,
    ))
}

fn parse_parenthesized_ident(suffix: &str, i: &mut usize) -> Result<String, SqlError> {
    let bytes = suffix.as_bytes();
    while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
        *i += 1;
    }
    if *i >= bytes.len() || bytes[*i] != b'(' {
        return Err(parse_err("expected '(' before partition key"));
    }
    *i += 1;
    while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
        *i += 1;
    }
    let start = *i;
    while *i < bytes.len()
        && (bytes[*i].is_ascii_alphanumeric() || bytes[*i] == b'_' || bytes[*i] == b'`')
    {
        *i += 1;
    }
    if start == *i {
        return Err(parse_err("expected partition key column"));
    }
    let mut col = String::from_utf8_lossy(&bytes[start..*i]).to_string();
    if col.starts_with('`') && col.ends_with('`') && col.len() >= 2 {
        col = col[1..col.len() - 1].to_string();
    }
    while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
        *i += 1;
    }
    if *i >= bytes.len() || bytes[*i] != b')' {
        return Err(parse_err("expected ')' after partition key"));
    }
    *i += 1;
    Ok(col)
}

fn parse_parenthesized_expr(suffix: &str, i: &mut usize) -> Result<String, SqlError> {
    let bytes = suffix.as_bytes();
    if *i >= bytes.len() || bytes[*i] != b'(' {
        return Err(parse_err("expected '('"));
    }
    *i += 1;
    let start = *i;
    let mut depth = 1i32;
    while *i < bytes.len() && depth > 0 {
        match bytes[*i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth > 0 {
            *i += 1;
        }
    }
    if depth != 0 {
        return Err(parse_err("unclosed '('"));
    }
    let inner = String::from_utf8_lossy(&bytes[start..*i]).to_string();
    *i += 1;
    Ok(inner)
}

fn extract_create_table_name(table_part: &str) -> Option<String> {
    let rest = table_part
        .strip_prefix("CREATE TABLE")
        .or_else(|| table_part.strip_prefix("create table"))?
        .trim();
    if rest.starts_with("IF NOT EXISTS") || rest.starts_with("if not exists") {
        let rest = rest
            .strip_prefix("IF NOT EXISTS")
            .or_else(|| rest.strip_prefix("if not exists"))?
            .trim();
        return extract_table_ident(rest);
    }
    extract_table_ident(rest)
}

fn extract_table_ident(rest: &str) -> Option<String> {
    let token: String = rest
        .chars()
        .take_while(|c| *c != '(' && !c.is_whitespace())
        .collect();
    if token.is_empty() {
        None
    } else {
        Some(token.trim_matches('`').to_string())
    }
}

fn parse_err(detail: &str) -> SqlError {
    SqlError::Parse(rusql_i18n::messages::sql_parse_error(detail))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn rewrite_range_partition_strips_clause() {
        let sql = "CREATE TABLE t (id INT PRIMARY KEY) PARTITION BY RANGE (id) (PARTITION p0 VALUES LESS THAN (10), PARTITION p1 VALUES LESS THAN MAXVALUE)";
        let rewritten = rewrite_create_table_range_partitions(sql).unwrap();
        assert!(!rewritten.to_ascii_uppercase().contains("PARTITION BY"));
        let pending = take_pending_range_partition().unwrap();
        assert_eq!(pending.table, "t");
        assert_eq!(pending.meta.key_column, "id");
        assert_eq!(pending.meta.partitions.len(), 2);
        let stmts = parse(&rewritten).unwrap();
        assert!(matches!(
            stmts[0],
            sqlparser::ast::Statement::CreateTable(_)
        ));
        let _ = take_pending_range_partition();
    }

    #[test]
    fn hash_partition_rejected() {
        let sql = "CREATE TABLE t (id INT) PARTITION BY HASH (id) PARTITIONS 4";
        let err = rewrite_create_table_range_partitions(sql).unwrap_err();
        assert!(err.to_string().contains("HASH") || err.to_string().contains("hash"));
    }
}

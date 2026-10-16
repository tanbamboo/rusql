//! MySQL `RENAME TABLE a TO b, c TO d` multi-pair parsing (M155).
//!
//! sqlparser 0.53 has no standalone `RENAME TABLE` statement; pairs are stored in thread-local
//! storage and the SQL is rewritten to `ALTER TABLE … RENAME TO …` for the first pair.

use std::cell::RefCell;

use crate::SqlError;

thread_local! {
    static PENDING_RENAME_TABLE_PAIRS: RefCell<Option<Vec<(String, String)>>> =
        const { RefCell::new(None) };
}

pub fn take_pending_rename_table_pairs() -> Option<Vec<(String, String)>> {
    PENDING_RENAME_TABLE_PAIRS.with(|p| p.borrow_mut().take())
}

fn trim_stmt(sql: &str) -> &str {
    sql.trim().trim_end_matches(';').trim()
}

fn unquote_ident(token: &str) -> String {
    let t = token.trim();
    if t.len() >= 2 && t.starts_with('`') && t.ends_with('`') {
        t[1..t.len() - 1].replace("``", "`")
    } else {
        t.to_string()
    }
}

/// Parse `RENAME TABLE t1 TO t2 [, t3 TO t4 …]` when the whole statement is a rename.
pub fn try_parse_rename_table_pairs(sql: &str) -> Option<Vec<(String, String)>> {
    let s = trim_stmt(sql);
    let upper = s.to_ascii_uppercase();
    if !upper.starts_with("RENAME TABLE ") {
        return None;
    }
    let rest = s[13..].trim();
    if rest.is_empty() {
        return None;
    }
    let mut pairs = Vec::new();
    let mut segment = rest;
    loop {
        let to_pos = segment.to_ascii_uppercase().find(" TO ")?;
        let old_part = segment[..to_pos].trim();
        if old_part.is_empty() {
            return None;
        }
        let after_to = segment[to_pos + 4..].trim_start();
        let (new_token, remainder) = split_ident_and_rest(after_to)?;
        if new_token.is_empty() {
            return None;
        }
        pairs.push((unquote_ident(old_part), unquote_ident(new_token)));
        segment = remainder.trim();
        if segment.is_empty() {
            break;
        }
        if !segment.starts_with(',') {
            return None;
        }
        segment = segment[1..].trim();
        if segment.is_empty() {
            return None;
        }
    }
    Some(pairs)
}

fn split_ident_and_rest(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    if s.starts_with('`') {
        let mut end = 1usize;
        let bytes = s.as_bytes();
        while end < bytes.len() {
            if bytes[end] == b'`' {
                if end + 1 < bytes.len() && bytes[end + 1] == b'`' {
                    end += 2;
                    continue;
                }
                end += 1;
                break;
            }
            end += 1;
        }
        let ident = &s[..end];
        return Some((ident, s[end..].trim_start()));
    }
    let end = s
        .find(|c: char| c == ',' || c.is_whitespace())
        .unwrap_or(s.len());
    Some((&s[..end], &s[end..]))
}

fn is_information_schema_table(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "information_schema"
        || lower.starts_with("information_schema.")
        || lower.split('.').any(|p| p == "information_schema")
}

fn quote_table_name(name: &str) -> String {
    if name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        && !name.is_empty()
    {
        name.to_string()
    } else {
        format!("`{}`", name.replace('`', "``"))
    }
}

/// If `sql` is a multi-pair `RENAME TABLE`, stash pairs and rewrite to sqlparser-friendly ALTER.
pub fn rewrite_rename_table_multi(sql: &str) -> Result<String, SqlError> {
    PENDING_RENAME_TABLE_PAIRS.with(|p| *p.borrow_mut() = None);
    let Some(pairs) = try_parse_rename_table_pairs(sql) else {
        return Ok(sql.to_string());
    };
    if pairs.is_empty() {
        return Err(SqlError::Parse(rusql_i18n::messages::sql_parse_error(
            "empty RENAME TABLE",
        )));
    }
    for (old, new) in &pairs {
        if is_information_schema_table(old) || is_information_schema_table(new) {
            return Err(SqlError::Parse(
                rusql_i18n::messages::sql_rename_information_schema_forbidden(),
            ));
        }
    }
    PENDING_RENAME_TABLE_PAIRS.with(|p| *p.borrow_mut() = Some(pairs.clone()));
    let (old, new) = &pairs[0];
    Ok(format!(
        "ALTER TABLE {} RENAME TO {}",
        quote_table_name(old),
        quote_table_name(new)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn parse_single_and_multi_pair_rename() {
        let pairs = try_parse_rename_table_pairs("RENAME TABLE t1 TO t2").unwrap();
        assert_eq!(pairs, vec![("t1".into(), "t2".into())]);

        let pairs = try_parse_rename_table_pairs("RENAME TABLE t1 TO t1_old, t2 TO t1").unwrap();
        assert_eq!(
            pairs,
            vec![("t1".into(), "t1_old".into()), ("t2".into(), "t1".into())]
        );
    }

    #[test]
    fn rewrite_sets_pending_and_parses() {
        let sql = "RENAME TABLE a TO b, c TO a";
        let rewritten = rewrite_rename_table_multi(sql).unwrap();
        assert!(rewritten.starts_with("ALTER TABLE a RENAME TO b"));
        let _ = take_pending_rename_table_pairs();
        let stmts = parse(sql).unwrap();
        assert_eq!(stmts.len(), 1);
        let pending = take_pending_rename_table_pairs().unwrap();
        assert_eq!(pending.len(), 2);
    }
}

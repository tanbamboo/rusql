//! `CREATE FULLTEXT INDEX` (M197). sqlparser 0.53 parses `CREATE INDEX` only.

use std::cell::Cell;

thread_local! {
    static CREATE_INDEX_FULLTEXT: Cell<bool> = const { Cell::new(false) };
}

/// Clear the flag at the start of [`crate::parse`] so an earlier statement cannot leak.
pub fn reset_create_index_fulltext() {
    CREATE_INDEX_FULLTEXT.with(|flag| flag.set(false));
}

/// Whether the last [`crate::parse`] rewrote `CREATE FULLTEXT INDEX` (single statement).
pub fn take_create_index_fulltext() -> bool {
    CREATE_INDEX_FULLTEXT.with(|flag| {
        let value = flag.get();
        flag.set(false);
        value
    })
}

/// Rewrite `CREATE FULLTEXT INDEX` to `CREATE INDEX` and remember the flag for the executor.
pub fn rewrite_create_fulltext_index(sql: &str) -> String {
    let Some(span) = find_create_fulltext_index(sql) else {
        return sql.to_string();
    };
    CREATE_INDEX_FULLTEXT.with(|flag| flag.set(true));
    let mut out = String::with_capacity(sql.len());
    out.push_str(&sql[..span.0]);
    out.push_str("CREATE INDEX");
    out.push_str(&sql[span.1..]);
    out
}

/// Byte span of `CREATE FULLTEXT INDEX`, keywords separated by whitespace, outside quotes.
fn find_create_fulltext_index(sql: &str) -> Option<(usize, usize)> {
    let bytes = sql.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\'' || bytes[i] == b'"' || bytes[i] == b'`' {
            i = skip_quoted(sql, i);
            continue;
        }
        if let Some(after_create) = keyword_at(sql, i, "CREATE") {
            let after_ws = skip_ws(sql, after_create);
            if let Some(after_fulltext) = keyword_at(sql, after_ws, "FULLTEXT") {
                let after_ws = skip_ws(sql, after_fulltext);
                if let Some(after_index) = keyword_at(sql, after_ws, "INDEX") {
                    return Some((i, after_index));
                }
            }
        }
        i += utf8_len(bytes[i]);
    }
    None
}

fn keyword_at(sql: &str, i: usize, keyword: &str) -> Option<usize> {
    let rest = sql.get(i..)?;
    let prefix = rest.get(..keyword.len())?;
    if !prefix.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let end = i + keyword.len();
    let boundary = sql[end..]
        .chars()
        .next()
        .map(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
        .unwrap_or(true);
    if boundary {
        Some(end)
    } else {
        None
    }
}

fn skip_ws(sql: &str, mut i: usize) -> usize {
    while let Some(ch) = sql[i..].chars().next() {
        if !ch.is_whitespace() {
            break;
        }
        i += ch.len_utf8();
    }
    i
}

fn skip_quoted(sql: &str, start: usize) -> usize {
    let quote = sql.as_bytes()[start];
    let mut i = start + 1;
    let bytes = sql.as_bytes();
    while i < bytes.len() {
        if bytes[i] == quote {
            if i + 1 < bytes.len() && bytes[i + 1] == quote {
                i += 2;
                continue;
            }
            return i + 1;
        }
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        i += utf8_len(bytes[i]);
    }
    sql.len()
}

fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first < 0xE0 {
        2
    } else if first < 0xF0 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn rewrite_create_fulltext_index_parses_as_create_index() {
        let sql = parse("CREATE FULLTEXT INDEX ft_body ON t (body)").unwrap();
        assert!(take_create_index_fulltext());
        let rendered = sql[0].to_string();
        assert!(
            rendered.to_ascii_uppercase().contains("CREATE INDEX"),
            "{rendered}"
        );
        assert!(!rendered.to_ascii_uppercase().contains("FULLTEXT"));
    }

    #[test]
    fn plain_create_index_is_not_fulltext() {
        let _ = parse("CREATE INDEX idx ON t (id)").unwrap();
        assert!(!take_create_index_fulltext());
    }

    #[test]
    fn rewrite_does_not_split_multibyte_literals() {
        let _ = parse("INSERT INTO t VALUES ('café')").unwrap();
        assert!(!take_create_index_fulltext());
    }
}

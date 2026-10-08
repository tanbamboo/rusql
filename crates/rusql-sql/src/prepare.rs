//! Rewrite MySQL text `PREPARE name FROM 'sql'` / `DROP PREPARE`
//! so sqlparser 0.53 can parse them (`PREPARE … AS` / `DEALLOCATE PREPARE`).

/// If `sql` contains MySQL text-prepare forms, rewrite them for sqlparser.
pub fn rewrite_text_prepare(sql: &str) -> Option<String> {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut start = 0;
    let mut changed = false;
    while start < sql.len() {
        let (stmt_end, semi) = next_statement_end(sql, start);
        let stmt = &sql[start..stmt_end];
        if let Some(rewritten) = rewrite_one(stmt) {
            out.push_str(&rewritten);
            changed = true;
        } else {
            out.push_str(stmt);
        }
        if semi {
            out.push(';');
            start = stmt_end + 1;
        } else {
            start = stmt_end;
        }
    }
    changed.then_some(out)
}

fn rewrite_one(stmt: &str) -> Option<String> {
    let leading = leading_ws(stmt);
    let rest = &stmt[leading.len()..];
    if let Some(rewritten) = rewrite_prepare_from(rest) {
        return Some(format!("{leading}{rewritten}"));
    }
    if let Some(rewritten) = rewrite_drop_prepare(rest) {
        return Some(format!("{leading}{rewritten}"));
    }
    None
}

fn rewrite_prepare_from(stmt: &str) -> Option<String> {
    let rest = skip_keyword(stmt, "PREPARE")?;
    let (name, rest) = take_ident(rest)?;
    let rest = skip_keyword(rest, "FROM")?;
    let (inner, rest) = take_sql_string(rest)?;
    if !rest.trim().is_empty() {
        return None;
    }
    let inner = inner.trim().trim_end_matches(';').trim();
    if inner.is_empty() || contains_top_level_semicolon(inner) {
        return None;
    }
    Some(format!("PREPARE {name} AS {inner}"))
}

fn rewrite_drop_prepare(stmt: &str) -> Option<String> {
    let rest = skip_keyword(stmt, "DROP")?;
    let rest = skip_keyword(rest, "PREPARE")?;
    let (name, rest) = take_ident(rest)?;
    if !rest.trim().is_empty() {
        return None;
    }
    Some(format!("DEALLOCATE PREPARE {name}"))
}

fn skip_keyword<'a>(s: &'a str, keyword: &str) -> Option<&'a str> {
    let s = s.trim_start();
    if s.len() < keyword.len() {
        return None;
    }
    if !s.get(..keyword.len())?.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let after = &s[keyword.len()..];
    if after.is_empty() || after.starts_with(|c: char| c.is_ascii_whitespace()) {
        Some(after)
    } else {
        None
    }
}

fn take_ident(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix('`') {
        let end = rest.find('`')?;
        let name = &rest[..end];
        if name.is_empty() {
            return None;
        }
        return Some((format!("`{name}`"), &rest[end + 1..]));
    }
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some((s[..end].to_string(), &s[end..]))
}

fn take_sql_string(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let bytes = s.as_bytes();
    let quote = *bytes.first()?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let mut i = 1;
    let mut out = String::new();
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            out.push(unescape_char(bytes[i + 1]));
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            if i + 1 < bytes.len() && bytes[i + 1] == quote {
                out.push(quote as char);
                i += 2;
                continue;
            }
            return Some((out, &s[i + 1..]));
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    None
}

fn unescape_char(b: u8) -> char {
    match b {
        b'n' => '\n',
        b't' => '\t',
        b'r' => '\r',
        b'0' => '\0',
        other => other as char,
    }
}

fn contains_top_level_semicolon(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(next) = skip_literal_or_comment(sql, i) {
            i = next;
            continue;
        }
        if bytes[i] == b';' {
            return true;
        }
        i += 1;
    }
    false
}

fn leading_ws(s: &str) -> &str {
    let n = s.len() - s.trim_start().len();
    &s[..n]
}

fn next_statement_end(sql: &str, start: usize) -> (usize, bool) {
    let bytes = sql.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        if let Some(next) = skip_literal_or_comment(sql, i) {
            i = next;
            continue;
        }
        if bytes[i] == b';' {
            return (i, true);
        }
        i += 1;
    }
    (sql.len(), false)
}

fn skip_literal_or_comment(sql: &str, i: usize) -> Option<usize> {
    let bytes = sql.as_bytes();
    if i >= bytes.len() {
        return None;
    }
    match bytes[i] {
        b'\'' | b'"' | b'`' => Some(skip_quoted(sql, i, bytes[i])),
        b'#' => Some(skip_line(sql, i + 1)),
        b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => Some(skip_line(sql, i + 2)),
        b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => Some(skip_block_comment(sql, i + 2)),
        _ => None,
    }
}

fn skip_quoted(sql: &str, start: usize, quote: u8) -> usize {
    let bytes = sql.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            if i + 1 < bytes.len() && bytes[i + 1] == quote {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    sql.len()
}

fn skip_line(sql: &str, mut i: usize) -> usize {
    let bytes = sql.as_bytes();
    while i < bytes.len() && bytes[i] != b'\n' {
        i += 1;
    }
    i.min(sql.len())
}

fn skip_block_comment(sql: &str, mut i: usize) -> usize {
    let bytes = sql.as_bytes();
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            return i + 2;
        }
        i += 1;
    }
    sql.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_prepare_from_literal() {
        assert_eq!(
            rewrite_text_prepare("PREPARE gap_stmt FROM 'SELECT 1'").as_deref(),
            Some("PREPARE gap_stmt AS SELECT 1")
        );
        assert_eq!(
            rewrite_text_prepare("prepare `Gap` from \"SELECT 2\";").as_deref(),
            Some("PREPARE `Gap` AS SELECT 2;")
        );
    }

    #[test]
    fn rewrite_drop_prepare() {
        assert_eq!(
            rewrite_text_prepare("DROP PREPARE gap_stmt").as_deref(),
            Some("DEALLOCATE PREPARE gap_stmt")
        );
    }

    #[test]
    fn rewrite_batched_prepare_execute() {
        assert_eq!(
            rewrite_text_prepare("PREPARE gap_stmt FROM 'SELECT 1'; EXECUTE gap_stmt").as_deref(),
            Some("PREPARE gap_stmt AS SELECT 1; EXECUTE gap_stmt")
        );
    }

    #[test]
    fn ignores_neighbors() {
        assert!(rewrite_text_prepare("SELECT 1").is_none());
        assert!(rewrite_text_prepare("DEALLOCATE PREPARE gap_stmt").is_none());
        assert!(rewrite_text_prepare("EXECUTE gap_stmt").is_none());
        assert!(rewrite_text_prepare("PREPARE gap_stmt FROM @sql").is_none());
        assert!(rewrite_text_prepare("PREPARE gap_stmt FROM 'SELECT 1; SELECT 2'").is_none());
    }
}

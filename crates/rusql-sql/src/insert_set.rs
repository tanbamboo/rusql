//! Rewrite MySQL `INSERT INTO t SET col=expr, …` to column-list `INSERT … VALUES (…)`.

const ASSIGNMENT_STOP: &[&str] = &["ON"];

/// Rewrite assignment-form INSERT statements so sqlparser can parse them.
pub fn rewrite_insert_set(sql: &str) -> Option<String> {
    let mut out = String::with_capacity(sql.len() + 32);
    let mut changed = false;
    let mut start = 0;
    while start < sql.len() {
        let (stmt_end, semi) = next_statement_end(sql, start);
        let stmt = &sql[start..stmt_end];
        if let Some(rewritten) = try_rewrite_insert_set_stmt(stmt) {
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

fn try_rewrite_insert_set_stmt(stmt: &str) -> Option<String> {
    let trimmed = stmt.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (mut i, _is_replace) = parse_dml_start(trimmed)?;
    i = skip_ws(trimmed, i);
    for kw in [
        "LOW_PRIORITY",
        "DELAYED",
        "HIGH_PRIORITY",
        "IGNORE",
        "INTO",
        "TABLE",
    ] {
        i = skip_ws(trimmed, i);
        i = skip_optional_keyword(trimmed, i, kw).unwrap_or(i);
    }
    i = skip_ws(trimmed, i);
    let table_end = parse_object_name_end(trimmed, i)?;
    i = skip_ws(trimmed, table_end);
    if trimmed.as_bytes().get(i) == Some(&b'(') {
        return None;
    }
    if keyword_at(trimmed, i, &["SELECT", "WITH", "DEFAULT", "VALUES"]) {
        return None;
    }
    i = skip_keyword(trimmed, i, "SET")?;
    i = skip_ws(trimmed, i);
    let (assign_end, columns, values) = parse_assignment_list(trimmed, i)?;
    if columns.is_empty() {
        return None;
    }
    let suffix = trimmed[assign_end..].trim_start();
    let lead_ws = stmt.find(trimmed)?;
    let prefix = &stmt[..lead_ws + table_end];
    let mut out = String::with_capacity(stmt.len() + 32);
    out.push_str(prefix);
    out.push_str(" (");
    for (idx, col) in columns.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(col);
    }
    out.push_str(") VALUES (");
    for (idx, val) in values.iter().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(val);
    }
    out.push(')');
    if !suffix.is_empty() {
        out.push(' ');
        out.push_str(suffix);
    }
    Some(out)
}

fn word_boundary_after(after: Option<u8>) -> bool {
    match after {
        None => true,
        Some(c) => !c.is_ascii_alphanumeric() && c != b'_',
    }
}

fn parse_dml_start(s: &str) -> Option<(usize, bool)> {
    if s.len() >= 7 && s[..7].eq_ignore_ascii_case("REPLACE") {
        let after = s.as_bytes().get(7).copied();
        if word_boundary_after(after) {
            return Some((7, true));
        }
    }
    if s.len() >= 6 && s[..6].eq_ignore_ascii_case("INSERT") {
        let after = s.as_bytes().get(6).copied();
        if word_boundary_after(after) {
            return Some((6, false));
        }
    }
    None
}

fn parse_assignment_list(s: &str, mut i: usize) -> Option<(usize, Vec<String>, Vec<String>)> {
    let mut columns = Vec::new();
    let mut values = Vec::new();
    loop {
        i = skip_ws(s, i);
        if i >= s.len() {
            break;
        }
        if keyword_at(s, i, ASSIGNMENT_STOP) {
            break;
        }
        let (col_end, col) = parse_column_ref(s, i)?;
        i = skip_ws(s, col_end);
        i = skip_eq(s, i)?;
        i = skip_ws(s, i);
        let val_start = i;
        let val_end = scan_expr_end(s, val_start);
        values.push(s[val_start..val_end].trim().to_string());
        columns.push(col);
        i = skip_ws(s, val_end);
        if i < s.len() && s.as_bytes()[i] == b',' {
            i += 1;
            continue;
        }
        break;
    }
    Some((i, columns, values))
}

fn parse_column_ref(s: &str, i: usize) -> Option<(usize, String)> {
    let b = s.as_bytes();
    if i >= b.len() {
        return None;
    }
    if b[i] == b'`' {
        let end = skip_quoted(s, i, b'`');
        return Some((end, s[i..end].to_string()));
    }
    let start = i;
    let mut j = i;
    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'$') {
        j += 1;
    }
    if j == start {
        return None;
    }
    Some((j, s[start..j].to_string()))
}

fn parse_object_name_end(s: &str, mut i: usize) -> Option<usize> {
    let (end, _) = parse_column_ref(s, i)?;
    i = end;
    while s.as_bytes().get(i) == Some(&b'.') {
        i += 1;
        i = skip_ws(s, i);
        let (part_end, _) = parse_column_ref(s, i)?;
        i = part_end;
    }
    Some(i)
}

fn skip_eq(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    if i >= b.len() || b[i] != b'=' {
        return None;
    }
    Some(i + 1)
}

fn scan_expr_end(s: &str, start: usize) -> usize {
    let bytes = s.as_bytes();
    let mut i = start;
    let mut depth = 0i32;
    while i < bytes.len() {
        if let Some(next) = skip_literal_or_comment(s, i) {
            i = next;
            continue;
        }
        let c = bytes[i];
        if c == b'(' {
            depth += 1;
            i += 1;
            continue;
        }
        if c == b')' {
            if depth == 0 {
                return i;
            }
            depth -= 1;
            i += 1;
            continue;
        }
        if depth == 0 && c == b',' {
            return i;
        }
        if depth == 0 && keyword_at(s, i, ASSIGNMENT_STOP) {
            return i;
        }
        i += 1;
    }
    i
}

fn skip_optional_keyword(s: &str, i: usize, kw: &str) -> Option<usize> {
    skip_keyword(s, i, kw)
}

fn skip_keyword(s: &str, i: usize, kw: &str) -> Option<usize> {
    if !keyword_at(s, i, &[kw]) {
        return None;
    }
    Some(i + kw.len())
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn keyword_at(s: &str, i: usize, keywords: &[&str]) -> bool {
    let bytes = s.as_bytes();
    if i > 0 {
        let prev = bytes[i - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return false;
        }
    }
    for kw in keywords {
        if i + kw.len() > s.len() {
            continue;
        }
        if s[i..i + kw.len()].eq_ignore_ascii_case(kw) {
            let after = i + kw.len();
            if after == s.len() {
                return true;
            }
            let next = bytes[after];
            if !next.is_ascii_alphanumeric() && next != b'_' {
                return true;
            }
        }
    }
    false
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
    i
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
    use crate::parse;
    use sqlparser::ast::Statement;

    #[test]
    fn rewrite_insert_set_to_values() {
        let out = rewrite_insert_set("INSERT INTO t SET id=1, n=2").unwrap();
        assert_eq!(out, "INSERT INTO t (id, n) VALUES (1, 2)");
        let stmts = parse(&out).unwrap();
        assert!(matches!(stmts[0], Statement::Insert(_)));
    }

    #[test]
    fn rewrite_insert_ignore_set() {
        let out = rewrite_insert_set("INSERT IGNORE INTO t SET id=1, n='a'").unwrap();
        assert_eq!(out, "INSERT IGNORE INTO t (id, n) VALUES (1, 'a')");
    }

    #[test]
    fn rewrite_replace_set() {
        let out = rewrite_insert_set("REPLACE INTO t SET id=1, n=2").unwrap();
        assert_eq!(out, "REPLACE INTO t (id, n) VALUES (1, 2)");
    }

    #[test]
    fn rewrite_insert_set_on_duplicate() {
        let sql = "INSERT INTO t SET id=1, n=2 ON DUPLICATE KEY UPDATE n=3";
        let out = rewrite_insert_set(sql).unwrap();
        assert_eq!(
            out,
            "INSERT INTO t (id, n) VALUES (1, 2) ON DUPLICATE KEY UPDATE n=3"
        );
        parse(&out).unwrap();
    }

    #[test]
    fn leaves_insert_values_untouched() {
        assert!(rewrite_insert_set("INSERT INTO t VALUES (1, 2)").is_none());
        assert!(rewrite_insert_set("INSERT INTO t (id) VALUES (1)").is_none());
        assert!(rewrite_insert_set("INSERT INTO dst SELECT id FROM src").is_none());
    }

    #[test]
    fn parse_insert_set_end_to_end() {
        let stmts = parse("INSERT INTO t SET id=1, n=2").unwrap();
        assert!(matches!(stmts[0], Statement::Insert(_)));
    }
}

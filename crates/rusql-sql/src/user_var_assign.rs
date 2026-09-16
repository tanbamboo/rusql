//! Rewrite MySQL `@foo := expr` so sqlparser 0.53 can parse assignment.

/// Sentinel function name used for `SELECT @foo := expr` assignment.
pub const USER_VAR_ASSIGN_FN: &str = "__rusql_user_var_assign";

const SELECT_STOP: &[&str] = &[
    "FROM",
    "WHERE",
    "GROUP",
    "HAVING",
    "ORDER",
    "LIMIT",
    "UNION",
    "EXCEPT",
    "INTERSECT",
    "INTO",
    "FOR",
    "WINDOW",
    "PROCEDURE",
    "AS",
];

/// Rewrite `SET @foo := expr` to `SET @foo = expr` and `SELECT @foo := expr`
/// to `__rusql_user_var_assign('foo', expr)`.
pub fn rewrite_user_var_assign(sql: &str) -> Option<String> {
    let with_set = rewrite_set_colon_eq(sql).unwrap_or_else(|| sql.to_string());
    let with_select = rewrite_select_assign(&with_set).unwrap_or(with_set);
    (with_select != sql).then_some(with_select)
}

fn rewrite_set_colon_eq(sql: &str) -> Option<String> {
    let mut out = String::with_capacity(sql.len());
    let mut changed = false;
    let mut start = 0;
    while start < sql.len() {
        let (stmt_end, semi) = next_statement_end(sql, start);
        let stmt = &sql[start..stmt_end];
        if statement_starts_with_set(stmt) {
            if let Some(rewritten) = replace_colon_eq_outside_strings(stmt) {
                out.push_str(&rewritten);
                changed = true;
            } else {
                out.push_str(stmt);
            }
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

fn rewrite_select_assign(sql: &str) -> Option<String> {
    let mut out = String::with_capacity(sql.len() + 32);
    let mut i = 0;
    let mut changed = false;
    while i < sql.len() {
        if let Some((name, at, rhs_start, rhs_end)) = find_user_var_assign(sql, i) {
            out.push_str(&sql[i..at]);
            let rhs = sql[rhs_start..rhs_end].trim();
            out.push_str(USER_VAR_ASSIGN_FN);
            out.push_str("('");
            out.push_str(&name);
            out.push_str("', ");
            out.push_str(rhs);
            out.push(')');
            i = rhs_end;
            changed = true;
        } else {
            out.push_str(&sql[i..]);
            break;
        }
    }
    changed.then_some(out)
}

fn find_user_var_assign(sql: &str, from: usize) -> Option<(String, usize, usize, usize)> {
    let bytes = sql.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if let Some(next) = skip_literal_or_comment(sql, i) {
            i = next;
            continue;
        }
        if bytes[i] == b'@' && (i + 1 >= bytes.len() || bytes[i + 1] != b'@') {
            let name_start = i + 1;
            let mut name_end = name_start;
            while name_end < bytes.len() && is_ident_byte(bytes[name_end]) {
                name_end += 1;
            }
            if name_end > name_start {
                let mut j = name_end;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j + 1 < bytes.len() && bytes[j] == b':' && bytes[j + 1] == b'=' {
                    let rhs_start = j + 2;
                    let rhs_end = scan_rhs_end(sql, rhs_start);
                    let name = sql[name_start..name_end].to_ascii_lowercase();
                    return Some((name, i, rhs_start, rhs_end));
                }
            }
        }
        i += 1;
    }
    None
}

fn scan_rhs_end(sql: &str, start: usize) -> usize {
    let bytes = sql.as_bytes();
    let mut i = start;
    let mut depth = 0i32;
    while i < bytes.len() {
        if let Some(next) = skip_literal_or_comment(sql, i) {
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
        if depth == 0 && (c == b',' || c == b';') {
            return i;
        }
        if depth == 0 && keyword_at(sql, i, SELECT_STOP) {
            return i;
        }
        i += 1;
    }
    i
}

fn keyword_at(sql: &str, i: usize, keywords: &[&str]) -> bool {
    let bytes = sql.as_bytes();
    if i > 0 {
        let prev = bytes[i - 1];
        if prev.is_ascii_alphanumeric() || prev == b'_' {
            return false;
        }
    }
    for kw in keywords {
        if i + kw.len() > sql.len() {
            continue;
        }
        if sql[i..i + kw.len()].eq_ignore_ascii_case(kw) {
            let after = i + kw.len();
            if after == sql.len() {
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

fn statement_starts_with_set(stmt: &str) -> bool {
    let trimmed = stmt.trim_start();
    if trimmed.len() < 3 || !trimmed[..3].eq_ignore_ascii_case("set") {
        return false;
    }
    match trimmed.as_bytes().get(3) {
        None => true,
        Some(c) => c.is_ascii_whitespace() || *c == b';',
    }
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

fn replace_colon_eq_outside_strings(stmt: &str) -> Option<String> {
    let bytes = stmt.as_bytes();
    let mut out = String::with_capacity(stmt.len());
    let mut i = 0;
    let mut changed = false;
    while i < bytes.len() {
        if let Some(next) = skip_literal_or_comment(stmt, i) {
            out.push_str(&stmt[i..next]);
            i = next;
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b':' && bytes[i + 1] == b'=' {
            out.push('=');
            i += 2;
            changed = true;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    changed.then_some(out)
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

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_select_assignment() {
        let out = rewrite_user_var_assign("SELECT @foo := 1").unwrap();
        assert!(out.contains("__rusql_user_var_assign('foo', 1)"), "{out}");
    }

    #[test]
    fn rewrite_set_colon_eq() {
        assert_eq!(
            rewrite_user_var_assign("SET @foo := 1").as_deref(),
            Some("SET @foo = 1")
        );
    }

    #[test]
    fn rewrite_skips_plain_select() {
        assert!(rewrite_user_var_assign("SELECT @foo").is_none());
        assert!(rewrite_user_var_assign("SET @foo = 1").is_none());
    }
}

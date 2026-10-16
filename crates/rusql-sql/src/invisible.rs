//! MySQL `INVISIBLE` column / index DDL (M150). sqlparser 0.53 has no native keyword.

use std::cell::Cell;

/// Internal COMMENT marker parsed into catalog `invisible` (not emitted in SHOW CREATE).
pub const INVISIBLE_COMMENT_MARKER: &str = "__RUSQL_INVISIBLE__";

thread_local! {
    static CREATE_INDEX_INVISIBLE: Cell<bool> = const { Cell::new(false) };
}

/// Whether the last `parse()` saw `CREATE INDEX … INVISIBLE` (single-statement batch).
pub fn take_create_index_invisible() -> bool {
    CREATE_INDEX_INVISIBLE.with(|c| {
        let v = c.get();
        c.set(false);
        v
    })
}

fn strip_trailing_index_invisible(sql: &str) -> (String, bool) {
    let trimmed = sql.trim();
    let (body, suffix) = trimmed
        .strip_suffix(';')
        .map(|b| (b.trim(), ";"))
        .unwrap_or((trimmed, ""));
    const SUFFIX: &str = " INVISIBLE";
    if body.len() >= SUFFIX.len() {
        let split = body.len() - SUFFIX.len();
        if body[split..].eq_ignore_ascii_case(SUFFIX) {
            let stripped = format!("{}{}", &body[..split], suffix);
            return (stripped, true);
        }
    }
    (sql.to_string(), false)
}

/// Rewrite `INVISIBLE` so the MySQL parser accepts DDL; column/index flags recovered in executor.
pub fn rewrite_invisible_ddl(sql: &str) -> String {
    CREATE_INDEX_INVISIBLE.with(|c| c.set(false));
    let upper = sql.trim_start().to_ascii_uppercase();
    if upper.starts_with("CREATE INDEX") || upper.starts_with("CREATE UNIQUE INDEX") {
        let (rewritten, invisible) = strip_trailing_index_invisible(sql);
        if invisible {
            CREATE_INDEX_INVISIBLE.with(|c| c.set(true));
        }
        return rewritten;
    }
    if upper.starts_with("CREATE TABLE") || upper.starts_with("ALTER TABLE") {
        return replace_invisible_with_comment_marker(sql);
    }
    sql.to_string()
}

fn replace_invisible_with_comment_marker(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 32);
    let bytes = sql.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i].is_ascii_alphabetic() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &sql[start..i];
            if word.eq_ignore_ascii_case("INVISIBLE") {
                out.push_str(" COMMENT '");
                out.push_str(INVISIBLE_COMMENT_MARKER);
                out.push('\'');
            } else {
                out.push_str(word);
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use sqlparser::ast::Statement;

    #[test]
    fn rewrite_create_table_invisible_parses() {
        let sql = "CREATE TABLE t (id INT PRIMARY KEY, n INT INVISIBLE)";
        let rewritten = rewrite_invisible_ddl(sql);
        assert!(rewritten.contains(INVISIBLE_COMMENT_MARKER));
        let stmts = parse(sql).unwrap();
        assert!(matches!(stmts[0], Statement::CreateTable(_)));
    }

    #[test]
    fn create_index_invisible_flag() {
        let sql = "CREATE INDEX idx ON t (n) INVISIBLE";
        let rewritten = rewrite_invisible_ddl(sql);
        assert!(!rewritten.to_ascii_uppercase().contains("INVISIBLE"));
        assert!(take_create_index_invisible());
        let _ = parse(sql).unwrap();
        assert!(take_create_index_invisible());
        assert!(!take_create_index_invisible());
    }

    #[test]
    fn parse_create_table_with_invisible_column() {
        let sql = "CREATE TABLE inv_t (id INT PRIMARY KEY, n INT INVISIBLE)";
        let stmts = parse(sql).unwrap();
        assert!(matches!(stmts[0], Statement::CreateTable(_)));
    }
}

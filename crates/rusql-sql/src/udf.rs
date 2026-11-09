//! Native UDF `.so` / `CREATE FUNCTION … SONAME` — security out-of-scope skip (M201).

use crate::SqlError;

/// If `sql` is `CREATE [AGGREGATE] FUNCTION … SONAME …`, return a documented parse error.
///
/// Loading arbitrary shared libraries is a trust boundary. rusql does not `dlopen` user
/// `.so` UDFs. SQL stored functions (`CREATE FUNCTION … BEGIN … END`, M63) are unchanged.
pub fn try_reject_udf_soname(sql: &str) -> Option<SqlError> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return None;
    }
    let upper = trimmed.to_ascii_uppercase();
    let is_create_fn = upper.starts_with("CREATE FUNCTION")
        || upper.starts_with("CREATE AGGREGATE FUNCTION")
        || upper.starts_with("CREATE OR REPLACE FUNCTION")
        || upper.starts_with("CREATE OR REPLACE AGGREGATE FUNCTION");
    if !is_create_fn {
        return None;
    }
    if contains_soname_keyword(trimmed) {
        return Some(SqlError::Parse(
            rusql_i18n::messages::sql_udf_soname_not_supported(),
        ));
    }
    None
}

/// True when `SONAME` appears as a SQL keyword outside string/backtick literals.
fn contains_soname_keyword(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\'' || b == b'"' || b == b'`' {
            let quote = b;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == quote {
                    if quote != b'`' && i + 1 < bytes.len() && bytes[i + 1] == quote {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if is_ident_start(b) {
            let start = i;
            i += 1;
            while i < bytes.len() && is_ident_cont(bytes[i]) {
                i += 1;
            }
            if sql[start..i].eq_ignore_ascii_case("SONAME") {
                return true;
            }
            continue;
        }
        i += 1;
    }
    false
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn udf_soname_rejected() {
        for sql in [
            "CREATE FUNCTION myfunc RETURNS INTEGER SONAME 'udf_example.so'",
            "CREATE AGGREGATE FUNCTION agg RETURNS REAL SONAME \"libudf.so\"",
            "create function f returns string soname 'x.so'",
            "CREATE FUNCTION weird RETURNS INT SONAME 'x.so'",
        ] {
            let err = try_reject_udf_soname(sql).expect("must reject SONAME form");
            let msg = err.to_string();
            assert!(
                msg.contains("SONAME") || msg.contains("UDF") || msg.contains("不支持"),
                "unexpected message for {sql}: {msg}"
            );
            let parse_err = parse(sql).expect_err("parse must reject SONAME");
            assert!(
                parse_err.to_string().contains("SONAME")
                    || parse_err.to_string().contains("UDF")
                    || parse_err.to_string().contains("不支持"),
                "parse message: {}",
                parse_err
            );
        }
    }

    #[test]
    fn stored_function_without_soname_not_rejected_here() {
        assert!(
            try_reject_udf_soname("CREATE FUNCTION f() RETURNS INT BEGIN RETURN 42; END").is_none()
        );
        assert!(try_reject_udf_soname("SELECT 1").is_none());
        assert!(try_reject_udf_soname("CREATE TABLE t (id INT)").is_none());
    }

    #[test]
    fn soname_inside_string_literal_ignored() {
        // Not a UDF form (no SONAME keyword); stored-function body string may mention it.
        assert!(try_reject_udf_soname(
            "CREATE FUNCTION f() RETURNS VARCHAR(20) BEGIN RETURN 'SONAME'; END"
        )
        .is_none());
    }
}

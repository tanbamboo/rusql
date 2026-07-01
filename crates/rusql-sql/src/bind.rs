//! Placeholder binding for prepared statements (MVP).

use crate::SqlError;

/// Count `?` placeholders outside single-quoted string literals.
pub fn count_placeholders(sql: &str) -> usize {
    let mut count = 0;
    let mut in_string = false;
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                } else {
                    in_string = false;
                }
            }
            continue;
        }
        if c == '\'' {
            in_string = true;
        } else if c == '?' {
            count += 1;
        }
    }
    count
}

/// Replace each `?` with a SQL literal (`NULL`, quoted string, or bare number).
pub fn bind_placeholders(sql: &str, params: &[Option<String>]) -> Result<String, SqlError> {
    let expected = count_placeholders(sql);
    if params.len() != expected {
        return Err(SqlError::Parse(format!(
            "expected {expected} parameters, got {}",
            params.len()
        )));
    }
    let mut out = String::with_capacity(sql.len() + params.len() * 8);
    let mut param_idx = 0;
    let mut in_string = false;
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    out.push(chars.next().unwrap());
                } else {
                    in_string = false;
                }
            }
            continue;
        }
        if c == '\'' {
            in_string = true;
            out.push(c);
        } else if c == '?' {
            let p = params
                .get(param_idx)
                .ok_or_else(|| SqlError::Parse("missing parameter".into()))?;
            param_idx += 1;
            match p {
                None => out.push_str("NULL"),
                Some(v) => {
                    if v.chars().all(|ch| ch.is_ascii_digit()) && !v.is_empty() {
                        out.push_str(v);
                    } else {
                        out.push('\'');
                        for ch in v.chars() {
                            if ch == '\'' {
                                out.push_str("''");
                            } else if ch == '\\' {
                                out.push_str("\\\\");
                            } else {
                                out.push(ch);
                            }
                        }
                        out.push('\'');
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    if param_idx != params.len() {
        return Err(SqlError::Parse("parameter count mismatch".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_skips_strings() {
        assert_eq!(count_placeholders("SELECT ? FROM t WHERE s = '?'"), 1);
    }

    #[test]
    fn bind_string_and_int() {
        let sql = bind_placeholders(
            "INSERT INTO t VALUES (?, ?)",
            &[Some("1".into()), Some("alice".into())],
        )
        .unwrap();
        assert_eq!(sql, "INSERT INTO t VALUES (1, 'alice')");
    }
}

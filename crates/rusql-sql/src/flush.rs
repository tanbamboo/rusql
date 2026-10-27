//! `FLUSH` statements (M187).
//!
//! Only `FLUSH PRIVILEGES` reloads the catalog. Other recognized options parse
//! and are rejected by the executor/server with a documented errno.

use crate::SqlError;

/// A `FLUSH` statement this slice recognizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlushStatement {
    /// Reload `{data_dir}/mysql.user.json` into the live privilege catalog.
    Privileges,
    /// Recognized MySQL `FLUSH` option that this slice does not run.
    Unsupported { option: String },
}

enum FlushOption {
    Privileges,
    Unsupported(String),
}

/// When `sql` is a `FLUSH` statement, parse it. Otherwise `None`.
pub fn try_parse_flush(sql: &str) -> Option<Result<FlushStatement, SqlError>> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    let rest = strip_keyword(trimmed, "FLUSH")?;
    Some(parse_flush_body(rest))
}

fn parse_flush_body(rest: &str) -> Result<FlushStatement, SqlError> {
    let rest = rest.trim_start();
    let rest = if let Some(after) = strip_keyword(rest, "NO_WRITE_TO_BINLOG") {
        after
    } else if let Some(after) = strip_keyword(rest, "LOCAL") {
        after
    } else {
        rest
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(flush_syntax_error());
    }
    // `FLUSH TABLES …` includes table lists and `WITH READ LOCK`. Those commas
    // are not flush-option separators, and this slice does not lock or close tables.
    if first_word_is(rest, "TABLES") {
        return Ok(FlushStatement::Unsupported {
            option: "TABLES".into(),
        });
    }
    let mut unsupported = None;
    let mut saw_privileges = false;
    for part in rest.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(flush_syntax_error());
        }
        match classify_flush_option(part)? {
            FlushOption::Privileges => saw_privileges = true,
            FlushOption::Unsupported(name) => {
                if unsupported.is_none() {
                    unsupported = Some(name);
                }
            }
        }
    }
    if let Some(option) = unsupported {
        return Ok(FlushStatement::Unsupported { option });
    }
    if saw_privileges {
        Ok(FlushStatement::Privileges)
    } else {
        Err(flush_syntax_error())
    }
}

fn classify_flush_option(part: &str) -> Result<FlushOption, SqlError> {
    let words: Vec<&str> = part.split_whitespace().collect();
    let key = words
        .iter()
        .map(|word| word.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join(" ");
    match key.as_str() {
        "PRIVILEGES" => Ok(FlushOption::Privileges),
        "HOSTS" | "STATUS" | "LOGS" | "USER_RESOURCES" | "OPTIMIZER_COSTS" | "DES_KEY_FILE"
        | "BINARY LOGS" | "ERROR LOGS" | "GENERAL LOGS" | "SLOW LOGS" | "ENGINE LOGS" => {
            Ok(FlushOption::Unsupported(key))
        }
        _ if key == "RELAY LOGS" || key.starts_with("RELAY LOGS ") => {
            Ok(FlushOption::Unsupported("RELAY LOGS".into()))
        }
        _ if words
            .first()
            .is_some_and(|word| word.eq_ignore_ascii_case("TABLES")) =>
        {
            Ok(FlushOption::Unsupported("TABLES".into()))
        }
        _ => Err(flush_syntax_error()),
    }
}

fn first_word_is(sql: &str, word: &str) -> bool {
    sql.split_whitespace()
        .next()
        .is_some_and(|token| token.eq_ignore_ascii_case(word))
}

fn strip_keyword<'a>(sql: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = sql.trim_start();
    if rest.len() < keyword.len() || !rest[..keyword.len()].eq_ignore_ascii_case(keyword) {
        return None;
    }
    let after = &rest[keyword.len()..];
    if after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(after)
}

fn flush_syntax_error() -> SqlError {
    SqlError::Parse(rusql_i18n::messages::sql_parse_error("FLUSH"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_privileges_forms_reload() {
        for sql in [
            "FLUSH PRIVILEGES",
            "flush privileges;",
            "  FLUSH   PRIVILEGES  ",
            "FLUSH LOCAL PRIVILEGES",
            "FLUSH NO_WRITE_TO_BINLOG PRIVILEGES",
            "FLUSH PRIVILEGES, PRIVILEGES",
        ] {
            assert!(
                matches!(
                    try_parse_flush(sql).unwrap().unwrap(),
                    FlushStatement::Privileges
                ),
                "{sql}"
            );
        }
    }

    #[test]
    fn flush_privileges_other_variants_are_unsupported() {
        let cases = [
            ("FLUSH TABLES", "TABLES"),
            ("FLUSH TABLES t", "TABLES"),
            ("FLUSH TABLES t1, t2", "TABLES"),
            ("FLUSH TABLES WITH READ LOCK", "TABLES"),
            ("FLUSH TABLES t WITH READ LOCK", "TABLES"),
            ("FLUSH LOCAL TABLES", "TABLES"),
            ("FLUSH HOSTS", "HOSTS"),
            ("FLUSH STATUS", "STATUS"),
            ("FLUSH LOGS", "LOGS"),
            ("FLUSH BINARY LOGS", "BINARY LOGS"),
            ("FLUSH RELAY LOGS FOR CHANNEL c", "RELAY LOGS"),
            ("FLUSH PRIVILEGES, STATUS", "STATUS"),
        ];
        for (sql, option) in cases {
            match try_parse_flush(sql).unwrap().unwrap() {
                FlushStatement::Unsupported { option: got } => assert_eq!(got, option, "{sql}"),
                other => panic!("{sql}: {other:?}"),
            }
        }
    }

    #[test]
    fn flush_privileges_unknown_is_syntax_error_and_other_sql_is_ignored() {
        assert!(try_parse_flush("FLUSH").unwrap().is_err());
        assert!(try_parse_flush("FLUSH NOT_A_REAL_OPTION").unwrap().is_err());
        assert!(try_parse_flush("SELECT 1").is_none());
        assert!(try_parse_flush("FLUSHPRIVILEGES").is_none());
    }
}

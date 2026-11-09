//! `CLONE LOCAL DATA DIRECTORY` / local `CLONE INSTANCE` subset (M200).

use crate::SqlError;

/// Parsed clone control statement (local data-dir snapshot only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloneStatement {
    /// Copy the server data directory into an empty destination path.
    LocalDataDirectory { path: String },
}

/// When `sql` is a CLONE statement, parse it; otherwise `None`.
pub fn try_parse_clone(sql: &str) -> Option<Result<CloneStatement, SqlError>> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return None;
    }
    let upper = trimmed.to_ascii_uppercase();
    if !upper.starts_with("CLONE ") {
        return None;
    }

    // Remote donor form is out of this slice.
    if upper.starts_with("CLONE INSTANCE FROM ") || upper.starts_with("CLONE INSTANCE FROM\t") {
        return Some(Err(SqlError::Parse(
            rusql_i18n::messages::sql_clone_remote_not_supported(),
        )));
    }

    if let Some(path) = parse_local_data_directory(trimmed, &upper) {
        return Some(path.map(|p| CloneStatement::LocalDataDirectory { path: p }));
    }

    Some(Err(SqlError::Parse(
        rusql_i18n::messages::sql_clone_syntax(),
    )))
}

fn parse_local_data_directory(original: &str, upper: &str) -> Option<Result<String, SqlError>> {
    // CLONE LOCAL DATA DIRECTORY [=] 'path'
    // CLONE INSTANCE DATA DIRECTORY [=] 'path'  (rusql local alias)
    let rest = if let Some(r) = upper.strip_prefix("CLONE LOCAL DATA DIRECTORY") {
        r
    } else if let Some(r) = upper.strip_prefix("CLONE INSTANCE DATA DIRECTORY") {
        r
    } else {
        return None;
    };

    let rest_orig = if upper.starts_with("CLONE LOCAL DATA DIRECTORY") {
        &original["CLONE LOCAL DATA DIRECTORY".len()..]
    } else {
        &original["CLONE INSTANCE DATA DIRECTORY".len()..]
    };

    let rest = rest.trim_start();
    let rest_orig = rest_orig.trim_start();
    let (rest, rest_orig) = if let Some(stripped) = rest.strip_prefix('=') {
        (stripped.trim_start(), rest_orig[1..].trim_start())
    } else {
        (rest, rest_orig)
    };

    if rest.is_empty() {
        return Some(Err(SqlError::Parse(
            rusql_i18n::messages::sql_clone_syntax(),
        )));
    }

    Some(parse_quoted_path(rest_orig))
}

fn parse_quoted_path(s: &str) -> Result<String, SqlError> {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() < 2 {
        return Err(SqlError::Parse(rusql_i18n::messages::sql_clone_syntax()));
    }
    let quote = bytes[0];
    if quote != b'\'' && quote != b'"' {
        return Err(SqlError::Parse(rusql_i18n::messages::sql_clone_syntax()));
    }
    let mut out = String::new();
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            if i + 1 < bytes.len() && bytes[i + 1] == quote {
                out.push(quote as char);
                i += 2;
                continue;
            }
            let tail = s[i + 1..].trim();
            if !tail.is_empty() {
                return Err(SqlError::Parse(rusql_i18n::messages::sql_clone_syntax()));
            }
            if out.is_empty() {
                return Err(SqlError::Parse(rusql_i18n::messages::sql_clone_syntax()));
            }
            return Ok(out);
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    Err(SqlError::Parse(rusql_i18n::messages::sql_clone_syntax()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_local_and_instance_alias() {
        assert_eq!(
            try_parse_clone("CLONE LOCAL DATA DIRECTORY = '/tmp/clone'")
                .unwrap()
                .unwrap(),
            CloneStatement::LocalDataDirectory {
                path: "/tmp/clone".into()
            }
        );
        assert_eq!(
            try_parse_clone("clone instance data directory '/data/replica'")
                .unwrap()
                .unwrap(),
            CloneStatement::LocalDataDirectory {
                path: "/data/replica".into()
            }
        );
    }

    #[test]
    fn rejects_remote_from() {
        let err = try_parse_clone(
            "CLONE INSTANCE FROM 'root'@'127.0.0.1':3306 IDENTIFIED BY 'x' DATA DIRECTORY = '/d'",
        )
        .unwrap()
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not supported") || msg.contains("不支持"),
            "unexpected: {msg}"
        );
    }

    #[test]
    fn ignores_unrelated_sql() {
        assert!(try_parse_clone("SELECT 1").is_none());
        assert!(try_parse_clone("START GROUP_REPLICATION").is_none());
    }
}

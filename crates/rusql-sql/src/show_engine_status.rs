//! Rewrite `SHOW ENGINE <name> STATUS` for sqlparser
//! (no dedicated statement in 0.53).
//!
//! `SHOW ENGINES` is rewritten separately (M88). `SHOW ENGINE … MUTEX` is not
//! accepted here.

/// Internal virtual table the executor recognizes for `SHOW ENGINE … STATUS`.
pub const ENGINE_STATUS_VIRTUAL_TABLE: &str = "__rusql_engine_status";

/// Parsed `SHOW ENGINE engine STATUS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowEngineStatus {
    pub engine: String,
}

/// If `sql` is `SHOW ENGINE ident STATUS` (optional trailing `;`).
pub fn parse_show_engine_status(sql: &str) -> Option<ShowEngineStatus> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    let rest = skip_keyword(rest, "ENGINE")?;
    let (engine, rest) = take_ident(rest)?;
    if engine.is_empty() {
        return None;
    }
    let rest = skip_keyword(rest, "STATUS")?;
    if !rest.trim().is_empty() {
        return None;
    }
    Some(ShowEngineStatus { engine })
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_engine_status(sql: &str) -> Option<String> {
    let parsed = parse_show_engine_status(sql)?;
    Some(format!(
        "SELECT * FROM {ENGINE_STATUS_VIRTUAL_TABLE} WHERE __engine__ = '{}'",
        escape_sql_string(&parsed.engine)
    ))
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
    if let Some(rest) = s.strip_prefix('`') {
        let end = rest.find('`')?;
        let name = rest[..end].to_string();
        Some((name, &rest[end + 1..]))
    } else {
        let n = s
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
            .unwrap_or(s.len());
        if n == 0 {
            return None;
        }
        Some((s[..n].to_string(), &s[n..]))
    }
}

fn escape_sql_string(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_engine_innodb_status() {
        assert_eq!(
            parse_show_engine_status("SHOW ENGINE INNODB STATUS"),
            Some(ShowEngineStatus {
                engine: "INNODB".into()
            })
        );
        assert_eq!(
            parse_show_engine_status("show engine innodb status;"),
            Some(ShowEngineStatus {
                engine: "innodb".into()
            })
        );
        assert_eq!(
            parse_show_engine_status("SHOW ENGINE `InnoDB` STATUS"),
            Some(ShowEngineStatus {
                engine: "InnoDB".into()
            })
        );
        assert_eq!(
            parse_show_engine_status("SHOW ENGINE MUSQL STATUS"),
            Some(ShowEngineStatus {
                engine: "MUSQL".into()
            })
        );
    }

    #[test]
    fn ignores_engines_mutex_and_neighbors() {
        assert!(parse_show_engine_status("SHOW ENGINES").is_none());
        assert!(parse_show_engine_status("SHOW STORAGE ENGINES").is_none());
        assert!(parse_show_engine_status("SHOW ENGINE INNODB MUTEX").is_none());
        assert!(parse_show_engine_status("SHOW ENGINE INNODB").is_none());
        assert!(parse_show_engine_status("SHOW TABLE STATUS").is_none());
        assert!(parse_show_engine_status("SHOW STATUS").is_none());
        assert!(parse_show_engine_status("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_engine_innodb_status_to_internal_select() {
        assert_eq!(
            rewrite_show_engine_status("SHOW ENGINE INNODB STATUS").as_deref(),
            Some("SELECT * FROM __rusql_engine_status WHERE __engine__ = 'INNODB'")
        );
    }
}

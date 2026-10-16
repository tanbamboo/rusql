//! MySQL XA transaction statements (M162).

use crate::SqlError;

/// Parsed XA control statement (single-branch xid only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XaStatement {
    Start { xid: String },
    End { xid: String },
    Prepare { xid: String },
    Commit { xid: String },
    Rollback { xid: String },
    Recover,
}

/// When `sql` is an XA statement, parse it; otherwise `None`.
pub fn try_parse_xa(sql: &str) -> Option<Result<XaStatement, SqlError>> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    if trimmed.len() < 3 {
        return None;
    }
    let upper = trimmed.to_ascii_uppercase();
    if !upper.starts_with("XA ") {
        return None;
    }
    Some(parse_xa_inner(trimmed))
}

fn parse_xa_inner(sql: &str) -> Result<XaStatement, SqlError> {
    let rest = sql[3..].trim_start();
    let (verb, tail) = next_word(rest).ok_or_else(|| parse_err("expected XA verb"))?;
    match verb.to_ascii_uppercase().as_str() {
        "START" => {
            reject_xa_modifiers(tail, &["JOIN", "RESUME"])?;
            let xid = parse_xid(tail)?;
            Ok(XaStatement::Start { xid })
        }
        "END" => {
            let (xid, tail) = parse_xid_with_rest(tail)?;
            reject_xa_modifiers(tail, &["SUSPEND"])?;
            Ok(XaStatement::End { xid })
        }
        "PREPARE" => {
            let xid = parse_xid(tail)?;
            Ok(XaStatement::Prepare { xid })
        }
        "COMMIT" => {
            let (xid, tail) = parse_xid_with_rest(tail)?;
            reject_one_phase(tail)?;
            Ok(XaStatement::Commit { xid })
        }
        "ROLLBACK" => {
            let xid = parse_xid(tail)?;
            Ok(XaStatement::Rollback { xid })
        }
        "RECOVER" => {
            if !tail.trim().is_empty() {
                let upper_tail = tail.trim().to_ascii_uppercase();
                if upper_tail.starts_with("CONVERT XID") {
                    return Err(parse_err("XA RECOVER CONVERT XID is not supported"));
                }
                return Err(parse_err("unexpected tokens after XA RECOVER"));
            }
            Ok(XaStatement::Recover)
        }
        other => Err(parse_err(&format!("unknown XA verb '{other}'"))),
    }
}

fn parse_err(detail: &str) -> SqlError {
    SqlError::Parse(rusql_i18n::messages::sql_parse_error(detail))
}

fn next_word(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    let end = s.find(|c: char| c.is_ascii_whitespace()).unwrap_or(s.len());
    Some((&s[..end], &s[end..]))
}

fn parse_xid(s: &str) -> Result<String, SqlError> {
    let (xid, tail) = parse_xid_with_rest(s)?;
    if !tail.trim().is_empty() {
        return Err(parse_err("unexpected tokens after xid"));
    }
    Ok(xid)
}

fn parse_xid_with_rest(s: &str) -> Result<(String, &str), SqlError> {
    let s = s.trim_start();
    if s.is_empty() {
        return Err(parse_err("xid expected"));
    }
    if s.starts_with('\'') {
        let mut escaped = false;
        for (i, ch) in s.char_indices().skip(1) {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '\'' {
                let xid = s[1..i].replace("\\'", "'");
                return Ok((xid, &s[i + 1..]));
            }
        }
        return Err(parse_err("unterminated quoted xid"));
    }
    if s.starts_with("X'") || s.starts_with("x'") {
        return Err(parse_err("hex xid form is not supported in this milestone"));
    }
    Err(parse_err("xid must be a quoted string"))
}

fn reject_xa_modifiers(tail: &str, keywords: &[&str]) -> Result<(), SqlError> {
    let upper = tail.trim().to_ascii_uppercase();
    for kw in keywords {
        if upper.starts_with(kw) {
            return Err(parse_err(&format!("{kw} is not supported")));
        }
    }
    Ok(())
}

fn reject_one_phase(tail: &str) -> Result<(), SqlError> {
    let upper = tail.trim().to_ascii_uppercase();
    if upper.starts_with("ONE") {
        return Err(parse_err("XA COMMIT ONE PHASE is not supported"));
    }
    if !upper.is_empty() {
        return Err(parse_err("unexpected tokens after xid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_xa_start_end_prepare_commit() {
        assert_eq!(
            try_parse_xa("XA START 'tx1'").unwrap().unwrap(),
            XaStatement::Start { xid: "tx1".into() }
        );
        assert_eq!(
            try_parse_xa("xa end 'tx1';").unwrap().unwrap(),
            XaStatement::End { xid: "tx1".into() }
        );
        assert_eq!(
            try_parse_xa("XA PREPARE 'tx1'").unwrap().unwrap(),
            XaStatement::Prepare { xid: "tx1".into() }
        );
        assert_eq!(
            try_parse_xa("XA COMMIT 'tx1'").unwrap().unwrap(),
            XaStatement::Commit { xid: "tx1".into() }
        );
        assert_eq!(
            try_parse_xa("XA ROLLBACK 'tx1'").unwrap().unwrap(),
            XaStatement::Rollback { xid: "tx1".into() }
        );
        assert_eq!(
            try_parse_xa("XA RECOVER").unwrap().unwrap(),
            XaStatement::Recover
        );
    }

    #[test]
    fn parse_xa_non_xa_returns_none() {
        assert!(try_parse_xa("BEGIN").is_none());
        assert!(try_parse_xa("SELECT 1").is_none());
    }

    #[test]
    fn parse_xa_rejects_join() {
        assert!(try_parse_xa("XA START 'x' JOIN").unwrap().is_err());
    }
}

//! Rewrite `SHOW GRANTS` for sqlparser.

/// Parsed `SHOW GRANTS` target account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowGrantsTarget {
    pub user: String,
    pub host: String,
}

/// Parse `SHOW GRANTS [FOR user@host]`.
pub fn parse_show_grants(sql: &str) -> Option<ShowGrantsTarget> {
    let s = sql.trim().trim_end_matches(';').trim();
    let upper = s.to_ascii_uppercase();
    if !upper.starts_with("SHOW ") {
        return None;
    }
    let rest = s[5..].trim_start();
    if !rest.to_ascii_uppercase().starts_with("GRANTS") {
        return None;
    }
    let rest = rest[6..].trim_start();
    let upper_rest = rest.to_ascii_uppercase();
    if upper_rest.starts_with("FOR ") {
        let account = rest[4..].trim();
        return Some(parse_account_literal(account));
    }
    None
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_grants(sql: &str) -> Option<String> {
    let target = parse_show_grants(sql)?;
    let account = format!("{}@{}", target.user, target.host).replace('\'', "''");
    Some(format!(
        "SELECT grant_line FROM __rusql_show_grants WHERE __account__ = '{account}'"
    ))
}

/// Rewrite `SHOW GRANTS` without `FOR` to the current session account.
pub fn rewrite_show_grants_current(sql: &str, user: &str, host: &str) -> Option<String> {
    let s = sql.trim().trim_end_matches(';').trim();
    let upper = s.to_ascii_uppercase();
    if upper != "SHOW GRANTS" {
        return None;
    }
    let account = format!("{user}@{host}").replace('\'', "''");
    Some(format!(
        "SELECT grant_line FROM __rusql_show_grants WHERE __account__ = '{account}'"
    ))
}

/// Normalize MySQL `'user'@'host'` account literals to `` `user@host` `` for sqlparser.
pub fn rewrite_mysql_account_literals(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if let Some((user, host, end)) = read_quoted_account(bytes, i) {
                out.push('`');
                out.push_str(&user);
                out.push('@');
                out.push_str(&host);
                out.push('`');
                i = end;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn read_quoted_account(bytes: &[u8], start: usize) -> Option<(String, String, usize)> {
    if bytes.get(start)? != &b'\'' {
        return None;
    }
    let (user, mut i) = read_quoted(bytes, start + 1)?;
    if bytes.get(i)? != &b'@' {
        return None;
    }
    i += 1;
    if bytes.get(i)? != &b'\'' {
        return None;
    }
    let (host, end) = read_quoted(bytes, i + 1)?;
    Some((user, host, end))
}

fn read_quoted(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    let mut value = String::new();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                if bytes.get(i + 1) == Some(&b'\'') {
                    value.push('\'');
                    i += 2;
                    continue;
                }
                return Some((value, i + 1));
            }
            b => {
                value.push(b as char);
                i += 1;
            }
        }
    }
    None
}

fn parse_account_literal(token: &str) -> ShowGrantsTarget {
    let token = token.trim();
    if token.starts_with('\'') {
        if let Some((user, host, _)) = read_quoted_account(token.as_bytes(), 0) {
            return ShowGrantsTarget { user, host };
        }
    }
    if token.starts_with('`') {
        let inner = token.trim_matches('`');
        if let Some((user, host)) = inner.split_once('@') {
            return ShowGrantsTarget {
                user: user.to_string(),
                host: host.to_string(),
            };
        }
        return ShowGrantsTarget {
            user: inner.to_string(),
            host: "%".into(),
        };
    }
    if let Some((user, host)) = token.split_once('@') {
        return ShowGrantsTarget {
            user: user.to_string(),
            host: host.to_string(),
        };
    }
    ShowGrantsTarget {
        user: token.to_string(),
        host: "%".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_show_grants_for_account() {
        let target = parse_show_grants("SHOW GRANTS FOR 'app'@'%'").unwrap();
        assert_eq!(target.user, "app");
        assert_eq!(target.host, "%");
    }

    #[test]
    fn rewrite_show_grants_sql() {
        let sql = super::rewrite_show_grants("SHOW GRANTS FOR app").unwrap();
        assert!(sql.contains("__rusql_show_grants"));
        assert!(sql.contains("'app@%'"));
    }

    #[test]
    fn rewrite_account_literals() {
        let sql = rewrite_mysql_account_literals("GRANT SELECT ON rusql.* TO 'app'@'%'");
        assert!(sql.contains("`app@%`"));
    }
}

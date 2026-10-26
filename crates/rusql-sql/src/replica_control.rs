//! `CHANGE MASTER TO` / `START SLAVE` / `STOP SLAVE` and 8.0.23+ synonyms (M179).

use crate::SqlError;

/// Options accepted by `CHANGE MASTER TO` / `CHANGE REPLICATION SOURCE TO`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChangeMasterOptions {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    /// Stored in replica catalog; never logged by callers.
    pub password: Option<String>,
    pub log_file: Option<String>,
    pub log_pos: Option<u64>,
}

/// Replica control statement (M179).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplicaControlStatement {
    ChangeMaster(ChangeMasterOptions),
    StartSlave,
    StopSlave,
}

/// When `sql` is a replica-control statement, parse it; otherwise `None`.
pub fn try_parse_replica_control(sql: &str) -> Option<Result<ReplicaControlStatement, SqlError>> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return None;
    }
    let upper = trimmed.to_ascii_uppercase();
    if upper.starts_with("CHANGE MASTER TO") || upper.starts_with("CHANGE REPLICATION SOURCE TO") {
        return Some(parse_change_master(trimmed));
    }
    if matches!(
        upper.as_str(),
        "START SLAVE" | "START REPLICA" | "STOP SLAVE" | "STOP REPLICA"
    ) {
        return Some(Ok(if upper.starts_with("START") {
            ReplicaControlStatement::StartSlave
        } else {
            ReplicaControlStatement::StopSlave
        }));
    }
    // Reject START/STOP with trailing clauses we do not support yet.
    if upper.starts_with("START SLAVE ")
        || upper.starts_with("START REPLICA ")
        || upper.starts_with("STOP SLAVE ")
        || upper.starts_with("STOP REPLICA ")
    {
        return Some(Err(parse_err(
            rusql_i18n::messages::sql_replica_start_stop_options(),
        )));
    }
    None
}

fn parse_change_master(sql: &str) -> Result<ReplicaControlStatement, SqlError> {
    let rest = if let Some(r) = strip_prefix_ci(sql, "CHANGE MASTER TO") {
        r
    } else if let Some(r) = strip_prefix_ci(sql, "CHANGE REPLICATION SOURCE TO") {
        r
    } else {
        return Err(parse_err(
            rusql_i18n::messages::sql_replica_change_requires_option(),
        ));
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return Err(parse_err(
            rusql_i18n::messages::sql_replica_change_requires_option(),
        ));
    }
    let mut opts = ChangeMasterOptions::default();
    for part in split_options(rest)? {
        apply_option(&mut opts, &part)?;
    }
    if opts.host.is_none()
        && opts.port.is_none()
        && opts.user.is_none()
        && opts.password.is_none()
        && opts.log_file.is_none()
        && opts.log_pos.is_none()
    {
        // Only ignored SSL options were supplied.
        return Err(parse_err(
            rusql_i18n::messages::sql_replica_change_requires_endpoint(),
        ));
    }
    Ok(ReplicaControlStatement::ChangeMaster(opts))
}

fn apply_option(opts: &mut ChangeMasterOptions, raw: &str) -> Result<(), SqlError> {
    let (key, value) = split_key_value(raw)?;
    let key_u = key.to_ascii_uppercase();
    match key_u.as_str() {
        "MASTER_HOST" | "SOURCE_HOST" => {
            opts.host = Some(parse_string_value(value)?);
        }
        "MASTER_PORT" | "SOURCE_PORT" => {
            opts.port = Some(parse_u16_value(value)?);
        }
        "MASTER_USER" | "SOURCE_USER" => {
            opts.user = Some(parse_string_value(value)?);
        }
        "MASTER_PASSWORD" | "SOURCE_PASSWORD" => {
            opts.password = Some(parse_string_value(value)?);
        }
        "MASTER_LOG_FILE" | "SOURCE_LOG_FILE" => {
            opts.log_file = Some(parse_string_value(value)?);
        }
        "MASTER_LOG_POS" | "SOURCE_LOG_POS" => {
            opts.log_pos = Some(parse_u64_value(value)?);
        }
        "MASTER_DELAY" | "SOURCE_DELAY" => {
            return Err(parse_err(rusql_i18n::messages::sql_replica_master_delay()));
        }
        "MASTER_AUTO_POSITION" | "SOURCE_AUTO_POSITION" => {
            return Err(parse_err(rusql_i18n::messages::sql_replica_auto_position()));
        }
        // SSL / TLS replica options: ignore until M181.
        k if k.starts_with("MASTER_SSL")
            || k.starts_with("SOURCE_SSL")
            || k == "GET_SOURCE_PUBLIC_KEY"
            || k == "MASTER_TLS_VERSION"
            || k == "SOURCE_TLS_VERSION"
            || k == "MASTER_PUBLIC_KEY_PATH"
            || k == "SOURCE_PUBLIC_KEY_PATH" => {}
        other => {
            return Err(parse_err(rusql_i18n::messages::sql_replica_unknown_option(
                other,
            )));
        }
    }
    Ok(())
}

fn split_options(s: &str) -> Result<Vec<String>, SqlError> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote: Option<char> = None;
    let mut escaped = false;
    for ch in s.chars() {
        if escaped {
            cur.push(ch);
            escaped = false;
            continue;
        }
        if let Some(q) = in_quote {
            cur.push(ch);
            if ch == '\\' {
                escaped = true;
            } else if ch == q {
                in_quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                in_quote = Some(ch);
                cur.push(ch);
            }
            ',' => {
                let part = cur.trim().to_string();
                if !part.is_empty() {
                    out.push(part);
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if in_quote.is_some() {
        return Err(parse_err(
            rusql_i18n::messages::sql_replica_unterminated_string(),
        ));
    }
    let part = cur.trim().to_string();
    if !part.is_empty() {
        out.push(part);
    }
    Ok(out)
}

fn split_key_value(s: &str) -> Result<(&str, &str), SqlError> {
    let eq = s
        .find('=')
        .ok_or_else(|| parse_err(rusql_i18n::messages::sql_replica_expected_key_value()))?;
    let key = s[..eq].trim();
    let value = s[eq + 1..].trim();
    if key.is_empty() || value.is_empty() {
        return Err(parse_err(
            rusql_i18n::messages::sql_replica_expected_key_value(),
        ));
    }
    Ok((key, value))
}

fn parse_string_value(s: &str) -> Result<String, SqlError> {
    let s = s.trim();
    if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
        let inner = &s[1..s.len() - 1];
        return Ok(inner.replace("\\'", "'").replace("\\\"", "\""));
    }
    // Unquoted identifiers / hostnames.
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' || c == ':')
    {
        return Ok(s.to_string());
    }
    Err(parse_err(
        rusql_i18n::messages::sql_replica_string_must_be_quoted(),
    ))
}

fn parse_u16_value(s: &str) -> Result<u16, SqlError> {
    let s = s.trim().trim_matches('\'').trim_matches('"');
    s.parse::<u16>()
        .map_err(|_| parse_err(rusql_i18n::messages::sql_replica_invalid_port(s)))
}

fn parse_u64_value(s: &str) -> Result<u64, SqlError> {
    let s = s.trim().trim_matches('\'').trim_matches('"');
    s.parse::<u64>()
        .map_err(|_| parse_err(rusql_i18n::messages::sql_replica_invalid_log_pos(s)))
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let s = s.trim_start();
    if s.len() < prefix.len() {
        return None;
    }
    if !s.get(..prefix.len())?.eq_ignore_ascii_case(prefix) {
        return None;
    }
    Some(&s[prefix.len()..])
}

fn parse_err(detail: impl AsRef<str>) -> SqlError {
    SqlError::Parse(rusql_i18n::messages::sql_parse_error(detail.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_change_master_basic() {
        let stmt = try_parse_replica_control(
            "CHANGE MASTER TO MASTER_HOST='127.0.0.1', MASTER_PORT=3307, MASTER_USER='repl'",
        )
        .unwrap()
        .unwrap();
        match stmt {
            ReplicaControlStatement::ChangeMaster(o) => {
                assert_eq!(o.host.as_deref(), Some("127.0.0.1"));
                assert_eq!(o.port, Some(3307));
                assert_eq!(o.user.as_deref(), Some("repl"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parse_change_replication_source_synonym() {
        let stmt = try_parse_replica_control(
            "CHANGE REPLICATION SOURCE TO SOURCE_HOST='db', SOURCE_PORT=3306, SOURCE_USER='u'",
        )
        .unwrap()
        .unwrap();
        match stmt {
            ReplicaControlStatement::ChangeMaster(o) => {
                assert_eq!(o.host.as_deref(), Some("db"));
                assert_eq!(o.port, Some(3306));
                assert_eq!(o.user.as_deref(), Some("u"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parse_start_stop_synonyms() {
        assert_eq!(
            try_parse_replica_control("START SLAVE").unwrap().unwrap(),
            ReplicaControlStatement::StartSlave
        );
        assert_eq!(
            try_parse_replica_control("START REPLICA").unwrap().unwrap(),
            ReplicaControlStatement::StartSlave
        );
        assert_eq!(
            try_parse_replica_control("STOP SLAVE").unwrap().unwrap(),
            ReplicaControlStatement::StopSlave
        );
        assert_eq!(
            try_parse_replica_control("stop replica;").unwrap().unwrap(),
            ReplicaControlStatement::StopSlave
        );
    }

    #[test]
    fn ignores_ssl_options() {
        let stmt = try_parse_replica_control(
            "CHANGE MASTER TO MASTER_HOST='h', MASTER_SSL=1, MASTER_USER='u'",
        )
        .unwrap()
        .unwrap();
        match stmt {
            ReplicaControlStatement::ChangeMaster(o) => {
                assert_eq!(o.host.as_deref(), Some("h"));
                assert_eq!(o.user.as_deref(), Some("u"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn rejects_master_delay_and_auto_position() {
        assert!(
            try_parse_replica_control("CHANGE MASTER TO MASTER_DELAY=10")
                .unwrap()
                .is_err()
        );
        assert!(
            try_parse_replica_control("CHANGE MASTER TO MASTER_AUTO_POSITION=1")
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn non_replica_returns_none() {
        assert!(try_parse_replica_control("SELECT 1").is_none());
        assert!(try_parse_replica_control("SHOW SLAVE STATUS").is_none());
    }
}

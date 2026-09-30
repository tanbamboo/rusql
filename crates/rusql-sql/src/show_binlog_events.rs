//! Rewrite `SHOW BINLOG EVENTS` for sqlparser (no dedicated statement in 0.53).
//!
//! MySQL 8.0: `SHOW BINLOG EVENTS [IN 'log_name'] [FROM pos] [LIMIT [offset,] row_count]`.

/// Internal virtual table the executor recognizes for `SHOW BINLOG EVENTS`.
pub const BINLOG_EVENTS_VIRTUAL_TABLE: &str = "__rusql_binlog_events";

/// Parsed `SHOW BINLOG EVENTS` clauses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowBinlogEvents {
    pub log_name: Option<String>,
    pub from_pos: Option<u32>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

/// If `sql` is `SHOW BINLOG EVENTS …`, return the parsed form.
pub fn parse_show_binlog_events(sql: &str) -> Option<ShowBinlogEvents> {
    let s = sql.trim().trim_end_matches(';').trim();
    let rest = skip_keyword(s, "SHOW")?;
    let rest = skip_keyword(rest, "BINLOG")?;
    let rest = skip_keyword(rest, "EVENTS")?;

    let mut rest = rest.trim_start();
    let mut log_name = None;
    if let Some(after) = skip_keyword(rest, "IN") {
        let (name, after) = take_log_name(after)?;
        if name.is_empty() {
            return None;
        }
        log_name = Some(name);
        rest = after.trim_start();
    }

    let mut from_pos = None;
    if let Some(after) = skip_keyword(rest, "FROM") {
        let (pos, after) = take_uint(after)?;
        let pos = u32::try_from(pos).ok()?;
        from_pos = Some(pos);
        rest = after.trim_start();
    }

    let mut offset = None;
    let mut limit = None;
    if let Some(after) = skip_keyword(rest, "LIMIT") {
        let (first, after) = take_uint(after)?;
        let after = after.trim_start();
        if let Some(after_comma) = after.strip_prefix(',') {
            let (count, after) = take_uint(after_comma)?;
            offset = Some(first);
            limit = Some(count);
            rest = after.trim_start();
        } else {
            limit = Some(first);
            rest = after.trim_start();
        }
    }

    if !rest.is_empty() {
        return None;
    }
    Some(ShowBinlogEvents {
        log_name,
        from_pos,
        offset,
        limit,
    })
}

/// Rewrite to an internal SELECT the executor recognizes.
pub fn rewrite_show_binlog_events(sql: &str) -> Option<String> {
    let parsed = parse_show_binlog_events(sql)?;
    let mut parts = Vec::new();
    if let Some(log) = &parsed.log_name {
        parts.push(format!("__log__ = '{}'", escape_sql_string(log)));
    }
    if let Some(pos) = parsed.from_pos {
        parts.push(format!("__from__ = '{pos}'"));
    }
    let mut out = if parts.is_empty() {
        format!("SELECT * FROM {BINLOG_EVENTS_VIRTUAL_TABLE}")
    } else {
        format!(
            "SELECT * FROM {BINLOG_EVENTS_VIRTUAL_TABLE} WHERE {}",
            parts.join(" AND ")
        )
    };
    if let Some(limit) = parsed.limit {
        out.push_str(&format!(" LIMIT {limit}"));
    }
    if let Some(offset) = parsed.offset {
        out.push_str(&format!(" OFFSET {offset}"));
    }
    Some(out)
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

fn take_log_name(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if let Some(rest) = take_string(s) {
        return Some(rest);
    }
    if let Some(rest) = s.strip_prefix('`') {
        let end = rest.find('`')?;
        let name = rest[..end].to_string();
        return Some((name, &rest[end + 1..]));
    }
    let n = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$'))
        .unwrap_or(s.len());
    if n == 0 {
        return None;
    }
    Some((s[..n].to_string(), &s[n..]))
}

fn take_string(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let rest = s.strip_prefix('\'')?;
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((i, ch)) = chars.next() {
        if ch == '\'' {
            if rest[i + ch.len_utf8()..].starts_with('\'') {
                out.push('\'');
                chars.next();
                continue;
            }
            return Some((out, &rest[i + ch.len_utf8()..]));
        }
        out.push(ch);
    }
    None
}

fn take_uint(s: &str) -> Option<(u64, &str)> {
    let s = s.trim_start();
    let n = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if n == 0 {
        return None;
    }
    let val = s[..n].parse().ok()?;
    Some((val, &s[n..]))
}

fn escape_sql_string(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_limit_in_from() {
        assert_eq!(
            parse_show_binlog_events("SHOW BINLOG EVENTS"),
            Some(ShowBinlogEvents {
                log_name: None,
                from_pos: None,
                offset: None,
                limit: None,
            })
        );
        assert_eq!(
            parse_show_binlog_events("show binlog events limit 1;"),
            Some(ShowBinlogEvents {
                log_name: None,
                from_pos: None,
                offset: None,
                limit: Some(1),
            })
        );
        assert_eq!(
            parse_show_binlog_events("SHOW BINLOG EVENTS LIMIT 2, 3"),
            Some(ShowBinlogEvents {
                log_name: None,
                from_pos: None,
                offset: Some(2),
                limit: Some(3),
            })
        );
        assert_eq!(
            parse_show_binlog_events("SHOW BINLOG EVENTS IN 'binlog.000001' FROM 4 LIMIT 1"),
            Some(ShowBinlogEvents {
                log_name: Some("binlog.000001".into()),
                from_pos: Some(4),
                offset: None,
                limit: Some(1),
            })
        );
        assert_eq!(
            parse_show_binlog_events("SHOW BINLOG EVENTS IN binlog.000001"),
            Some(ShowBinlogEvents {
                log_name: Some("binlog.000001".into()),
                from_pos: None,
                offset: None,
                limit: None,
            })
        );
    }

    #[test]
    fn ignores_neighbors() {
        assert!(parse_show_binlog_events("SHOW BINARY LOGS").is_none());
        assert!(parse_show_binlog_events("SHOW MASTER LOGS").is_none());
        assert!(parse_show_binlog_events("SHOW MASTER STATUS").is_none());
        assert!(parse_show_binlog_events("SHOW EVENTS").is_none());
        assert!(parse_show_binlog_events("SHOW ENGINES").is_none());
        assert!(parse_show_binlog_events("SHOW BINLOG EVENTS WHERE Pos = 4").is_none());
        assert!(parse_show_binlog_events("SELECT 1").is_none());
    }

    #[test]
    fn rewrite_to_internal_select() {
        assert_eq!(
            rewrite_show_binlog_events("SHOW BINLOG EVENTS").as_deref(),
            Some("SELECT * FROM __rusql_binlog_events")
        );
        assert_eq!(
            rewrite_show_binlog_events("SHOW BINLOG EVENTS LIMIT 1").as_deref(),
            Some("SELECT * FROM __rusql_binlog_events LIMIT 1")
        );
        let sql =
            rewrite_show_binlog_events("SHOW BINLOG EVENTS IN 'binlog.000001' FROM 4 LIMIT 2, 1")
                .unwrap();
        assert!(sql.contains("__rusql_binlog_events"));
        assert!(sql.contains("__log__ = 'binlog.000001'"));
        assert!(sql.contains("__from__ = '4'"));
        assert!(sql.contains("LIMIT 1"));
        assert!(sql.contains("OFFSET 2"));
    }
}

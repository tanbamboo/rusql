//! Parse CREATE PROCEDURE / CALL / CREATE TRIGGER / CREATE FUNCTION / CREATE EVENT / DROP (MVP).
use rusql_core::{
    EventMeta, FunctionMeta, ProcedureMeta, TriggerEvent, TriggerMeta, TriggerTiming,
    DEFAULT_SCHEMA,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredProgramStmt {
    CreateProcedure(ProcedureMeta),
    CreateFunction(FunctionMeta),
    DropProcedure {
        schema: String,
        name: String,
        if_exists: bool,
    },
    DropFunction {
        schema: String,
        name: String,
        if_exists: bool,
    },
    Call {
        schema: String,
        name: String,
    },
    CreateTrigger(TriggerMeta),
    DropTrigger {
        schema: String,
        name: String,
        if_exists: bool,
    },
    CreateEvent {
        meta: EventMeta,
        if_not_exists: bool,
    },
    DropEvent {
        schema: String,
        name: String,
        if_exists: bool,
    },
    AlterEvent {
        schema: String,
        name: String,
        schedule_type: Option<String>,
        execute_at: Option<String>,
        interval_value: Option<String>,
        interval_field: Option<String>,
        status: Option<String>,
        rename_schema: Option<String>,
        rename_name: Option<String>,
        body: Option<String>,
        starts: Option<String>,
        ends: Option<String>,
        definer: Option<String>,
        on_completion: Option<String>,
        comment: Option<String>,
    },
}

pub fn try_parse_stored_program(sql: &str) -> Option<StoredProgramStmt> {
    let t = sql.trim().trim_end_matches(';').trim();
    let u = t.to_ascii_uppercase();
    if u.starts_with("CREATE PROCEDURE") {
        return parse_create_procedure(t);
    }
    if u.starts_with("CREATE FUNCTION") {
        return parse_create_function(t);
    }
    if u.starts_with("DROP PROCEDURE") {
        return parse_drop_procedure(t);
    }
    if u.starts_with("DROP FUNCTION") {
        return parse_drop_function(t);
    }
    if u.starts_with("CALL ") {
        return parse_call(t);
    }
    if u.starts_with("CREATE TRIGGER") {
        return parse_create_trigger(t);
    }
    if u.starts_with("DROP TRIGGER") {
        return parse_drop_trigger(t);
    }
    if u.starts_with("CREATE EVENT") || u.starts_with("CREATE DEFINER") {
        return parse_create_event(t);
    }
    if u.starts_with("DROP EVENT") {
        return parse_drop_event(t);
    }
    if u.starts_with("ALTER EVENT") || u.starts_with("ALTER DEFINER") {
        return parse_alter_event(t);
    }
    None
}

pub fn procedure_meta_from_stmt(s: &StoredProgramStmt) -> Option<&ProcedureMeta> {
    match s {
        StoredProgramStmt::CreateProcedure(m) => Some(m),
        _ => None,
    }
}
pub fn trigger_meta_from_stmt(s: &StoredProgramStmt) -> Option<&TriggerMeta> {
    match s {
        StoredProgramStmt::CreateTrigger(m) => Some(m),
        _ => None,
    }
}

fn strip_bt(s: &str) -> &str {
    s.trim().trim_matches('`')
}

fn parse_create_procedure(input: &str) -> Option<StoredProgramStmt> {
    let rest = input.get(16..)?.trim_start();
    let paren = rest.find('(')?;
    let (schema, name) = split_qualified(strip_bt(&rest[..paren]))?;
    Some(StoredProgramStmt::CreateProcedure(ProcedureMeta {
        schema,
        name,
        body: extract_body(input)?,
    }))
}

pub fn function_meta_from_stmt(s: &StoredProgramStmt) -> Option<&FunctionMeta> {
    match s {
        StoredProgramStmt::CreateFunction(m) => Some(m),
        _ => None,
    }
}

fn parse_create_function(input: &str) -> Option<StoredProgramStmt> {
    let upper = input.to_ascii_uppercase();
    let returns_pos = upper.find(" RETURNS ")?;
    let begin_pos = upper.find(" BEGIN")?;
    let head = input.get(16..returns_pos)?.trim();
    let paren = head.find('(')?;
    let (schema, name) = split_qualified(strip_bt(&head[..paren]))?;
    let return_type = strip_function_characteristics(&input[returns_pos + 9..begin_pos]);
    let body = extract_body(input)?;
    let return_expr = extract_return_expr(&body)?;
    Some(StoredProgramStmt::CreateFunction(FunctionMeta {
        schema,
        name,
        return_type,
        return_expr,
    }))
}

/// Drop DETERMINISTIC / SQL SECURITY clauses so mysql-diff CREATE FUNCTION can
/// satisfy Docker MySQL 8.0 binlog rules without persisting characteristics.
fn strip_function_characteristics(raw: &str) -> String {
    let upper = raw.to_ascii_uppercase();
    const STOPS: &[&str] = &[
        " DETERMINISTIC",
        " NOT DETERMINISTIC",
        " NO SQL",
        " READS SQL DATA",
        " MODIFIES SQL DATA",
        " CONTAINS SQL",
        " SQL SECURITY",
        " COMMENT",
    ];
    let mut end = raw.len();
    for stop in STOPS {
        if let Some(i) = upper.find(stop) {
            end = end.min(i);
        }
    }
    raw[..end].trim().to_string()
}

fn extract_return_expr(body: &[String]) -> Option<String> {
    body.iter().find_map(|stmt| {
        let trimmed = stmt.trim();
        if trimmed.to_ascii_uppercase().starts_with("RETURN") {
            Some(trimmed[6..].trim().trim_end_matches(';').trim().to_string())
        } else {
            None
        }
    })
}

fn parse_drop_function(input: &str) -> Option<StoredProgramStmt> {
    let if_exists = input.to_ascii_uppercase().contains("IF EXISTS");
    let tokens: Vec<_> = input.split_whitespace().collect();
    let idx = tokens
        .iter()
        .position(|t| t.eq_ignore_ascii_case("FUNCTION"))?;
    let (schema, name) = split_qualified(strip_bt(tokens.get(idx + 1)?))?;
    Some(StoredProgramStmt::DropFunction {
        schema,
        name,
        if_exists,
    })
}

fn parse_drop_procedure(input: &str) -> Option<StoredProgramStmt> {
    let if_exists = input.to_ascii_uppercase().contains("IF EXISTS");
    let tokens: Vec<_> = input.split_whitespace().collect();
    let idx = tokens
        .iter()
        .position(|t| t.eq_ignore_ascii_case("PROCEDURE"))?;
    let (schema, name) = split_qualified(strip_bt(tokens.get(idx + 1)?))?;
    Some(StoredProgramStmt::DropProcedure {
        schema,
        name,
        if_exists,
    })
}

fn parse_call(input: &str) -> Option<StoredProgramStmt> {
    let rest = input.get(4..)?.trim_start();
    let (schema, name) = split_qualified(strip_bt(rest.split('(').next()?))?;
    Some(StoredProgramStmt::Call { schema, name })
}

fn parse_create_trigger(input: &str) -> Option<StoredProgramStmt> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.len() < 7 {
        return None;
    }
    let name = strip_bt(tokens[2]).to_string();
    let timing = match tokens[3].to_ascii_uppercase().as_str() {
        "BEFORE" => TriggerTiming::Before,
        "AFTER" => TriggerTiming::After,
        _ => return None,
    };
    let event = match tokens[4].to_ascii_uppercase().as_str() {
        "INSERT" => TriggerEvent::Insert,
        "UPDATE" => TriggerEvent::Update,
        "DELETE" => TriggerEvent::Delete,
        _ => return None,
    };
    if !tokens[5].eq_ignore_ascii_case("ON") {
        return None;
    }
    let table = strip_bt(tokens[6]).to_string();
    let body = extract_trigger_body(input)?;
    Some(StoredProgramStmt::CreateTrigger(TriggerMeta {
        schema: DEFAULT_SCHEMA.into(),
        name,
        table,
        timing,
        event,
        body,
    }))
}

fn extract_trigger_body(input: &str) -> Option<Vec<String>> {
    let upper = input.to_ascii_uppercase();
    let row_marker = "FOR EACH ROW";
    let idx = upper.find(row_marker)?;
    let after = input[idx + row_marker.len()..].trim();
    if after.to_ascii_uppercase().starts_with("BEGIN") {
        return extract_body(input);
    }
    let stmt = after.trim_end_matches(';').trim();
    if stmt.is_empty() {
        return None;
    }
    Some(vec![stmt.to_string()])
}

fn parse_drop_trigger(input: &str) -> Option<StoredProgramStmt> {
    let if_exists = input.to_ascii_uppercase().contains("IF EXISTS");
    let tokens: Vec<_> = input.split_whitespace().collect();
    let idx = tokens
        .iter()
        .position(|t| t.eq_ignore_ascii_case("TRIGGER"))?;
    let (schema, name) = split_qualified(strip_bt(tokens.get(idx + 1)?))?;
    Some(StoredProgramStmt::DropTrigger {
        schema,
        name,
        if_exists,
    })
}

const INTERVAL_UNITS: &[&str] = &["SECOND", "MINUTE", "HOUR", "DAY", "WEEK", "MONTH", "YEAR"];

struct EventSchedule {
    schedule_type: String,
    execute_at: Option<String>,
    interval_value: Option<String>,
    interval_field: Option<String>,
    starts: Option<String>,
    ends: Option<String>,
}

fn parse_create_event(input: &str) -> Option<StoredProgramStmt> {
    let rest = skip_keyword(input, "CREATE")?;
    let (definer, rest) = take_optional_definer(rest);
    let rest = skip_keyword(rest, "EVENT")?;
    let (if_not_exists, rest) = match skip_keyword(rest, "IF") {
        Some(after_if) => {
            let after_not = skip_keyword(after_if, "NOT")?;
            let after_exists = skip_keyword(after_not, "EXISTS")?;
            (true, after_exists)
        }
        None => (false, rest),
    };
    let (first, rest) = take_ident(rest)?;
    let rest = rest.trim_start();
    let (schema, name, rest) = if let Some(after_dot) = rest.strip_prefix('.') {
        let (second, rest) = take_ident(after_dot)?;
        (first, second, rest)
    } else {
        (DEFAULT_SCHEMA.to_string(), first, rest)
    };
    let rest = skip_keyword(rest, "ON")?;
    let rest = skip_keyword(rest, "SCHEDULE")?;
    let (schedule, rest) = parse_schedule(rest)?;
    let (on_completion, rest) = take_on_completion(rest);
    let (status, rest) = if let Some(after) = skip_keyword(rest, "ENABLE") {
        ("ENABLED".to_string(), after)
    } else if let Some(after) = skip_keyword(rest, "DISABLE") {
        ("DISABLED".to_string(), after)
    } else {
        ("ENABLED".to_string(), rest)
    };
    let (comment, rest) = take_optional_event_comment(rest)?;
    let rest = skip_keyword(rest, "DO")?;
    let body = rest.trim().trim_end_matches(';').trim();
    if body.is_empty() {
        return None;
    }
    Some(StoredProgramStmt::CreateEvent {
        meta: EventMeta {
            schema,
            name,
            schedule_type: schedule.schedule_type,
            execute_at: schedule.execute_at,
            interval_value: schedule.interval_value,
            interval_field: schedule.interval_field,
            status,
            body: body.to_string(),
            last_executed: None,
            starts: schedule.starts,
            ends: schedule.ends,
            definer,
            on_completion,
            comment,
        },
        if_not_exists,
    })
}

fn parse_drop_event(input: &str) -> Option<StoredProgramStmt> {
    let if_exists = input.to_ascii_uppercase().contains("IF EXISTS");
    let rest = skip_keyword(input, "DROP")?;
    let rest = skip_keyword(rest, "EVENT")?;
    let rest = if if_exists {
        let rest = skip_keyword(rest, "IF")?;
        skip_keyword(rest, "EXISTS")?
    } else {
        rest
    };
    let (first, rest) = take_ident(rest)?;
    let rest = rest.trim_start();
    let (schema, name, rest) = if let Some(after_dot) = rest.strip_prefix('.') {
        let (second, rest) = take_ident(after_dot)?;
        (first, second, rest)
    } else {
        (DEFAULT_SCHEMA.to_string(), first, rest)
    };
    if !rest.trim().is_empty() {
        return None;
    }
    Some(StoredProgramStmt::DropEvent {
        schema,
        name,
        if_exists,
    })
}

fn parse_schedule(rest: &str) -> Option<(EventSchedule, &str)> {
    if let Some(after_at) = skip_keyword(rest, "AT") {
        let (ts, rest) = take_quoted_string(after_at)?;
        return Some((
            EventSchedule {
                schedule_type: "ONE TIME".to_string(),
                execute_at: Some(ts),
                interval_value: None,
                interval_field: None,
                starts: None,
                ends: None,
            },
            rest,
        ));
    }
    let after_every = skip_keyword(rest, "EVERY")?;
    let (value, rest) = take_number(after_every)?;
    let (unit, rest) = take_ident(rest)?;
    let unit = unit.to_ascii_uppercase();
    if !INTERVAL_UNITS.iter().any(|u| *u == unit) {
        return None;
    }
    let (starts, ends, rest) = take_starts_ends(rest);
    Some((
        EventSchedule {
            schedule_type: "RECURRING".to_string(),
            execute_at: None,
            interval_value: Some(value),
            interval_field: Some(unit),
            starts,
            ends,
        },
        rest,
    ))
}

fn take_starts_ends(rest: &str) -> (Option<String>, Option<String>, &str) {
    let mut starts = None;
    let mut ends = None;
    let mut rest = rest;
    if let Some(after) = skip_keyword(rest, "STARTS") {
        if let Some((ts, after)) = take_quoted_string(after) {
            starts = Some(ts);
            rest = after;
        }
    }
    if let Some(after) = skip_keyword(rest, "ENDS") {
        if let Some((ts, after)) = take_quoted_string(after) {
            ends = Some(ts);
            rest = after;
        }
    }
    (starts, ends, rest)
}

fn parse_qualified_ident(rest: &str) -> Option<(String, String, &str)> {
    let (first, rest) = take_ident(rest)?;
    let rest = rest.trim_start();
    if let Some(after_dot) = rest.strip_prefix('.') {
        let (second, rest) = take_ident(after_dot)?;
        Some((first, second, rest))
    } else {
        Some((DEFAULT_SCHEMA.to_string(), first, rest))
    }
}

fn parse_alter_event(input: &str) -> Option<StoredProgramStmt> {
    let rest = skip_keyword(input, "ALTER")?;
    let (mut definer, rest) = take_optional_definer(rest);
    let rest = skip_keyword(rest, "EVENT")?;
    let (schema, name, mut rest) = parse_qualified_ident(rest)?;
    let mut schedule_type = None;
    let mut execute_at = None;
    let mut interval_value = None;
    let mut interval_field = None;
    let mut status = None;
    let mut rename_schema = None;
    let mut rename_name = None;
    let mut body = None;
    let mut starts = None;
    let mut ends = None;
    let mut on_completion = None;
    let mut comment = None;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        if let Some(after_on) = skip_keyword(rest, "ON") {
            if let Some(after) = skip_keyword(after_on, "SCHEDULE") {
                let (schedule, after) = parse_schedule(after)?;
                schedule_type = Some(schedule.schedule_type);
                execute_at = schedule.execute_at;
                interval_value = schedule.interval_value;
                interval_field = schedule.interval_field;
                if schedule.starts.is_some() {
                    starts = schedule.starts;
                }
                if schedule.ends.is_some() {
                    ends = schedule.ends;
                }
                rest = after;
                continue;
            }
            let after = skip_keyword(after_on, "COMPLETION")?;
            let (value, after) = parse_on_completion_value(after)?;
            on_completion = Some(value);
            rest = after;
            continue;
        }
        let (maybe_definer, after) = take_optional_definer(rest);
        if maybe_definer.is_some() {
            definer = maybe_definer;
            rest = after;
            continue;
        }
        if let Some(after_rename) = skip_keyword(rest, "RENAME") {
            let after = skip_keyword(after_rename, "TO")?;
            let (rs, rn, after) = parse_qualified_ident(after)?;
            rename_schema = Some(rs);
            rename_name = Some(rn);
            rest = after;
            continue;
        }
        if let Some(after) = skip_keyword(rest, "ENABLE") {
            status = Some("ENABLED".to_string());
            rest = after;
            continue;
        }
        if let Some(after) = skip_keyword(rest, "DISABLE") {
            status = Some("DISABLED".to_string());
            rest = after;
            continue;
        }
        if let Some(after) = skip_keyword(rest, "STARTS") {
            let (ts, after) = take_quoted_string(after)?;
            starts = Some(ts);
            rest = after;
            continue;
        }
        if let Some(after) = skip_keyword(rest, "ENDS") {
            let (ts, after) = take_quoted_string(after)?;
            ends = Some(ts);
            rest = after;
            continue;
        }
        if let Some(after) = skip_keyword(rest, "COMMENT") {
            let (text, after) = take_quoted_string(after)?;
            comment = Some(text);
            rest = after;
            continue;
        }
        if let Some(after) = skip_keyword(rest, "DO") {
            let stmt = after.trim().trim_end_matches(';').trim();
            if stmt.is_empty() {
                return None;
            }
            body = Some(stmt.to_string());
            rest = "";
            continue;
        }
        return None;
    }
    if schedule_type.is_none()
        && status.is_none()
        && rename_name.is_none()
        && body.is_none()
        && starts.is_none()
        && ends.is_none()
        && definer.is_none()
        && on_completion.is_none()
        && comment.is_none()
    {
        return None;
    }
    Some(StoredProgramStmt::AlterEvent {
        schema,
        name,
        schedule_type,
        execute_at,
        interval_value,
        interval_field,
        status,
        rename_schema,
        rename_name,
        body,
        starts,
        ends,
        definer,
        on_completion,
        comment,
    })
}

fn take_optional_event_comment(rest: &str) -> Option<(Option<String>, &str)> {
    if let Some(after) = skip_keyword(rest, "COMMENT") {
        let (text, after) = take_quoted_string(after)?;
        let comment = if text.is_empty() { None } else { Some(text) };
        Some((comment, after))
    } else {
        Some((None, rest))
    }
}

fn take_optional_definer(rest: &str) -> (Option<String>, &str) {
    let rest_trim = rest.trim_start();
    let Some(after) = skip_definer_kw(rest_trim) else {
        return (None, rest);
    };
    match take_account(after) {
        Some((account, after)) => (Some(account), after),
        None => (None, rest),
    }
}

fn skip_definer_kw(s: &str) -> Option<&str> {
    let s = s.trim_start();
    if s.len() < 7 || !s.get(..7)?.eq_ignore_ascii_case("DEFINER") {
        return None;
    }
    let after = &s[7..];
    if let Some(after) = after.strip_prefix('=') {
        return Some(after.trim_start());
    }
    if after.starts_with(|c: char| c.is_ascii_whitespace()) {
        let after = after.trim_start();
        return Some(after.strip_prefix('=').unwrap_or(after).trim_start());
    }
    None
}

fn take_ident_or_quoted(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if s.starts_with('\'') || s.starts_with('"') {
        take_quoted_string(s)
    } else {
        take_ident(s)
    }
}

fn take_account(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if s.len() >= 12 && s.get(..12)?.eq_ignore_ascii_case("CURRENT_USER") {
        let after = s[12..].trim_start();
        let after = after.strip_prefix("()").unwrap_or(after);
        return Some(("CURRENT_USER".to_string(), after));
    }
    let (user, rest) = take_ident_or_quoted(s)?;
    let rest = rest.trim_start().strip_prefix('@')?;
    let (host, rest) = take_ident_or_quoted(rest)?;
    Some((format!("{user}@{host}"), rest))
}

fn take_on_completion(rest: &str) -> (Option<String>, &str) {
    let rest_trim = rest.trim_start();
    let Some(after_on) = skip_keyword(rest_trim, "ON") else {
        return (None, rest);
    };
    let Some(after) = skip_keyword(after_on, "COMPLETION") else {
        return (None, rest);
    };
    match parse_on_completion_value(after) {
        Some((value, after)) => (Some(value), after),
        None => (None, rest),
    }
}

fn parse_on_completion_value(rest: &str) -> Option<(String, &str)> {
    if let Some(after_not) = skip_keyword(rest, "NOT") {
        let after = skip_keyword(after_not, "PRESERVE")?;
        return Some(("NOT PRESERVE".to_string(), after));
    }
    let after = skip_keyword(rest, "PRESERVE")?;
    Some(("PRESERVE".to_string(), after))
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
        if name.is_empty() {
            return None;
        }
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

fn take_quoted_string(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let quote = s.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let rest = &s[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some((rest[..end].to_string(), &rest[end + quote.len_utf8()..]))
}

fn take_number(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let n = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if n == 0 {
        return None;
    }
    Some((s[..n].to_string(), &s[n..]))
}

fn split_qualified(name: &str) -> Option<(String, String)> {
    if let Some((s, n)) = name.rsplit_once('.') {
        Some((s.into(), n.into()))
    } else {
        Some((DEFAULT_SCHEMA.into(), name.into()))
    }
}

fn extract_body(input: &str) -> Option<Vec<String>> {
    let begin = input.to_ascii_uppercase().find("BEGIN")?;
    let end = input.to_ascii_uppercase().rfind("END")?;
    Some(
        input[begin + 5..end]
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_create_function() {
        let stmt = try_parse_stored_program("CREATE FUNCTION f() RETURNS INT BEGIN RETURN 42; END")
            .unwrap();
        let StoredProgramStmt::CreateFunction(meta) = stmt else {
            panic!("expected create function");
        };
        assert_eq!(meta.name, "f");
        assert_eq!(meta.return_type, "INT");
        assert_eq!(meta.return_expr, "42");

        let stmt = try_parse_stored_program(
            "CREATE FUNCTION f() RETURNS INT DETERMINISTIC BEGIN RETURN 42; END",
        )
        .unwrap();
        let StoredProgramStmt::CreateFunction(meta) = stmt else {
            panic!("expected create function");
        };
        assert_eq!(meta.return_type, "INT");
        assert_eq!(meta.return_expr, "42");
    }

    #[test]
    fn parse_create_event_at_every_and_drop() {
        let stmt = try_parse_stored_program(
            "CREATE EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' DO SELECT 1",
        )
        .unwrap();
        let StoredProgramStmt::CreateEvent {
            meta,
            if_not_exists,
        } = stmt
        else {
            panic!("expected create event");
        };
        assert!(!if_not_exists);
        assert_eq!(meta.name, "e");
        assert_eq!(meta.schedule_type, "ONE TIME");
        assert_eq!(meta.execute_at.as_deref(), Some("2038-01-01 00:00:00"));
        assert_eq!(meta.status, "ENABLED");
        assert_eq!(meta.body, "SELECT 1");
        assert!(meta.comment.is_none());

        let stmt = try_parse_stored_program(
            "CREATE EVENT IF NOT EXISTS rusql.`e` ON SCHEDULE EVERY 1 HOUR DISABLE DO SELECT 1",
        )
        .unwrap();
        let StoredProgramStmt::CreateEvent {
            meta,
            if_not_exists,
        } = stmt
        else {
            panic!("expected create event");
        };
        assert!(if_not_exists);
        assert_eq!(meta.schema, "rusql");
        assert_eq!(meta.schedule_type, "RECURRING");
        assert_eq!(meta.interval_value.as_deref(), Some("1"));
        assert_eq!(meta.interval_field.as_deref(), Some("HOUR"));
        assert_eq!(meta.status, "DISABLED");
        assert!(meta.starts.is_none());
        assert!(meta.ends.is_none());

        let stmt = try_parse_stored_program(
            "CREATE EVENT e ON SCHEDULE EVERY 1 HOUR STARTS '2026-09-19 12:00:00' ENDS '2026-09-20 12:00:00' DO SELECT 1",
        )
        .unwrap();
        let StoredProgramStmt::CreateEvent { meta, .. } = stmt else {
            panic!("expected create event");
        };
        assert_eq!(meta.starts.as_deref(), Some("2026-09-19 12:00:00"));
        assert_eq!(meta.ends.as_deref(), Some("2026-09-20 12:00:00"));
        assert_eq!(meta.schedule_type, "RECURRING");

        let stmt = try_parse_stored_program(
            "CREATE DEFINER=`app`@`%` EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' ON COMPLETION PRESERVE DO SELECT 1",
        )
        .unwrap();
        let StoredProgramStmt::CreateEvent { meta, .. } = stmt else {
            panic!("expected create event");
        };
        assert_eq!(meta.definer.as_deref(), Some("app@%"));
        assert_eq!(meta.on_completion.as_deref(), Some("PRESERVE"));

        let stmt = try_parse_stored_program(
            "CREATE EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' ON COMPLETION NOT PRESERVE DO SELECT 1",
        )
        .unwrap();
        let StoredProgramStmt::CreateEvent { meta, .. } = stmt else {
            panic!("expected create event");
        };
        assert!(meta.definer.is_none());
        assert_eq!(meta.on_completion.as_deref(), Some("NOT PRESERVE"));

        let stmt = try_parse_stored_program(
            "CREATE EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' COMMENT 'hello' DO SELECT 1",
        )
        .unwrap();
        let StoredProgramStmt::CreateEvent { meta, .. } = stmt else {
            panic!("expected create event");
        };
        assert_eq!(meta.comment.as_deref(), Some("hello"));

        let stmt = try_parse_stored_program(
            "CREATE EVENT e ON SCHEDULE AT '2038-01-01 00:00:00' ON COMPLETION PRESERVE ENABLE COMMENT 'c' DO SELECT 1",
        )
        .unwrap();
        let StoredProgramStmt::CreateEvent { meta, .. } = stmt else {
            panic!("expected create event");
        };
        assert!(meta.definer.is_none());
        assert_eq!(meta.on_completion.as_deref(), Some("PRESERVE"));
        assert_eq!(meta.status, "ENABLED");
        assert_eq!(meta.comment.as_deref(), Some("c"));

        let stmt = try_parse_stored_program("DROP EVENT IF EXISTS e").unwrap();
        let StoredProgramStmt::DropEvent {
            name, if_exists, ..
        } = stmt
        else {
            panic!("expected drop event");
        };
        assert_eq!(name, "e");
        assert!(if_exists);

        assert!(try_parse_stored_program("SHOW CREATE EVENT e").is_none());
        assert!(try_parse_stored_program("SHOW EVENTS").is_none());
        assert!(
            try_parse_stored_program("CREATE FUNCTION f() RETURNS INT BEGIN RETURN 1; END")
                .is_some()
        );
    }

    #[test]
    fn parse_alter_event_schedule_status_rename_do() {
        let stmt = try_parse_stored_program("ALTER EVENT e ON SCHEDULE EVERY 1 DAY").unwrap();
        let StoredProgramStmt::AlterEvent {
            name,
            schedule_type,
            interval_value,
            interval_field,
            ..
        } = stmt
        else {
            panic!("expected alter event");
        };
        assert_eq!(name, "e");
        assert_eq!(schedule_type.as_deref(), Some("RECURRING"));
        assert_eq!(interval_value.as_deref(), Some("1"));
        assert_eq!(interval_field.as_deref(), Some("DAY"));

        let stmt = try_parse_stored_program("ALTER EVENT e DISABLE").unwrap();
        let StoredProgramStmt::AlterEvent { status, .. } = stmt else {
            panic!("expected alter event");
        };
        assert_eq!(status.as_deref(), Some("DISABLED"));

        let stmt = try_parse_stored_program("ALTER EVENT e RENAME TO e2 DO SELECT 2").unwrap();
        let StoredProgramStmt::AlterEvent {
            rename_name, body, ..
        } = stmt
        else {
            panic!("expected alter event");
        };
        assert_eq!(rename_name.as_deref(), Some("e2"));
        assert_eq!(body.as_deref(), Some("SELECT 2"));

        let stmt = try_parse_stored_program(
            "ALTER EVENT e ON SCHEDULE EVERY 1 HOUR STARTS '2026-09-19 12:00:00' ENDS '2026-09-20 12:00:00'",
        )
        .unwrap();
        let StoredProgramStmt::AlterEvent {
            starts,
            ends,
            schedule_type,
            ..
        } = stmt
        else {
            panic!("expected alter event");
        };
        assert_eq!(schedule_type.as_deref(), Some("RECURRING"));
        assert_eq!(starts.as_deref(), Some("2026-09-19 12:00:00"));
        assert_eq!(ends.as_deref(), Some("2026-09-20 12:00:00"));

        let stmt = try_parse_stored_program(
            "ALTER EVENT e STARTS '2026-01-01 00:00:00' ENDS '2026-12-31 00:00:00'",
        )
        .unwrap();
        let StoredProgramStmt::AlterEvent {
            starts,
            ends,
            schedule_type,
            ..
        } = stmt
        else {
            panic!("expected alter event");
        };
        assert!(schedule_type.is_none());
        assert_eq!(starts.as_deref(), Some("2026-01-01 00:00:00"));
        assert_eq!(ends.as_deref(), Some("2026-12-31 00:00:00"));

        let stmt = try_parse_stored_program("ALTER EVENT e ON COMPLETION PRESERVE").unwrap();
        let StoredProgramStmt::AlterEvent { on_completion, .. } = stmt else {
            panic!("expected alter event");
        };
        assert_eq!(on_completion.as_deref(), Some("PRESERVE"));

        let stmt = try_parse_stored_program("ALTER DEFINER=`app`@`%` EVENT e").unwrap();
        let StoredProgramStmt::AlterEvent { definer, .. } = stmt else {
            panic!("expected alter event");
        };
        assert_eq!(definer.as_deref(), Some("app@%"));

        let stmt = try_parse_stored_program("ALTER EVENT e COMMENT 'x'").unwrap();
        let StoredProgramStmt::AlterEvent { comment, .. } = stmt else {
            panic!("expected alter event");
        };
        assert_eq!(comment.as_deref(), Some("x"));

        assert!(try_parse_stored_program("ALTER EVENT e").is_none());
        assert!(try_parse_stored_program("ALTER TABLE t ADD id INT").is_none());
    }
}

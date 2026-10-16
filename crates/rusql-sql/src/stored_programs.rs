//! Parse CREATE PROCEDURE / CALL / CREATE TRIGGER / CREATE FUNCTION / CREATE EVENT / DROP (MVP).
use rusql_core::{
    EventMeta, FunctionMeta, ParameterMeta, ProcedureMeta, TriggerEvent, TriggerMeta,
    TriggerTiming, DEFAULT_SCHEMA,
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
        args: Vec<String>,
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
        disable_on_slave: Option<bool>,
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
    let close = matching_paren(rest, paren)?;
    let parameters = parse_procedure_parameters(&rest[paren + 1..close])?;
    let after_params = rest[close + 1..].trim();
    Some(StoredProgramStmt::CreateProcedure(ProcedureMeta {
        schema,
        name,
        body: extract_procedure_body(input, after_params)?,
        parameters,
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
    let paren = rest.find('(')?;
    let (schema, name) = split_qualified(strip_bt(&rest[..paren]))?;
    let close = matching_paren(rest, paren)?;
    let args = split_call_args(&rest[paren + 1..close])?;
    Some(StoredProgramStmt::Call { schema, name, args })
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
    let (status, disable_on_slave, rest) = match take_event_status(rest) {
        Some((status, disable_on_slave, after)) => (status, disable_on_slave, after),
        None => ("ENABLED".to_string(), false, rest),
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
            disable_on_slave,
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
    let mut disable_on_slave = None;
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
        if let Some((next_status, on_slave, after)) = take_event_status(rest) {
            status = Some(next_status);
            disable_on_slave = Some(on_slave);
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
        && disable_on_slave.is_none()
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
        disable_on_slave,
    })
}

fn take_optional_on_slave(rest: &str) -> (bool, &str) {
    let Some(after_on) = skip_keyword(rest, "ON") else {
        return (false, rest);
    };
    match skip_keyword(after_on, "SLAVE") {
        Some(after) => (true, after),
        None => (false, rest),
    }
}

/// `ENABLE` / `DISABLE` / `DISABLE ON SLAVE` / `ENABLE ON SLAVE` (M130).
/// `ENABLE ON SLAVE` is accepted and clears the replica-disable flag.
fn take_event_status(rest: &str) -> Option<(String, bool, &str)> {
    if let Some(after) = skip_keyword(rest, "ENABLE") {
        let (_, after) = take_optional_on_slave(after);
        return Some(("ENABLED".to_string(), false, after));
    }
    if let Some(after) = skip_keyword(rest, "DISABLE") {
        let (on_slave, after) = take_optional_on_slave(after);
        if on_slave {
            return Some(("ENABLED".to_string(), true, after));
        }
        return Some(("DISABLED".to_string(), false, after));
    }
    None
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

fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.get(open) != Some(&b'(') {
        return None;
    }
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'\'' | b'"' | b'`' => quote = Some(b),
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn split_top_level(s: &str, sep: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => quote = Some(c),
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c == sep && depth == 0 => {
                parts.push(s[start..i].trim().to_string());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(s[start..].trim().to_string());
    parts
}

/// Parse `(IN x INT, OUT y INT, INOUT z INT, …)` parameter lists (M132 / M165).
fn parse_procedure_parameters(src: &str) -> Option<Vec<ParameterMeta>> {
    let src = src.trim();
    if src.is_empty() {
        return Some(Vec::new());
    }
    let parts = split_top_level(src, ',');
    let mut out = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            return None;
        }
        let (mode, rest) = if let Some(r) = skip_keyword(part, "INOUT") {
            ("INOUT", r)
        } else if let Some(r) = skip_keyword(part, "OUT") {
            ("OUT", r)
        } else if let Some(r) = skip_keyword(part, "IN") {
            ("IN", r)
        } else {
            ("IN", part.as_str())
        };
        let rest = rest.trim();
        let (name, rest) = take_ident(rest)?;
        let data_type = rest.trim();
        if data_type.is_empty() {
            return None;
        }
        out.push(ParameterMeta {
            name,
            mode: mode.to_string(),
            data_type: data_type.to_string(),
            ordinal_position: (i as u32) + 1,
        });
    }
    Some(out)
}

fn split_call_args(src: &str) -> Option<Vec<String>> {
    let src = src.trim();
    if src.is_empty() {
        return Some(Vec::new());
    }
    let parts = split_top_level(src, ',');
    if parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    Some(parts)
}

/// Parsed `SIGNAL` statement inside a stored program body (M166).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalInfo {
    pub sqlstate: String,
    pub message_text: Option<String>,
    pub mysql_errno: Option<u16>,
}

/// True when `sql` is a `RESIGNAL` statement (possibly with `SET` clause).
pub fn is_resignal_statement(sql: &str) -> bool {
    let t = sql.trim().trim_end_matches(';').trim();
    t.to_ascii_uppercase().starts_with("RESIGNAL")
}

/// Parse `SIGNAL SQLSTATE '…' [SET …]` when present; `None` if not a SIGNAL statement.
pub fn try_parse_signal(sql: &str) -> Option<SignalInfo> {
    let rest = sql.trim().trim_end_matches(';').trim();
    let upper = rest.to_ascii_uppercase();
    if !upper.starts_with("SIGNAL ") {
        return None;
    }
    let mut pos = rest.get(7..)?.trim_start();
    if !pos.to_ascii_uppercase().starts_with("SQLSTATE ") {
        return None;
    }
    pos = pos.get(9..)?.trim_start();
    let (sqlstate, after_state) = parse_quoted_value(pos)?;
    if sqlstate.len() != 5 {
        return None;
    }
    let mut message_text = None;
    let mut mysql_errno = None;
    let mut tail = after_state.trim_start();
    if tail.to_ascii_uppercase().starts_with("SET ") {
        tail = tail.get(4..)?.trim_start();
        for item in split_signal_items(tail) {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let eq = item.find('=')?;
            let key = item[..eq].trim();
            let val = item[eq + 1..].trim();
            match key.to_ascii_uppercase().as_str() {
                "MESSAGE_TEXT" => {
                    message_text = Some(parse_signal_value(val)?);
                }
                "MYSQL_ERRNO" => {
                    mysql_errno = Some(parse_signal_errno(val)?);
                }
                _ => {}
            }
        }
    }
    Some(SignalInfo {
        sqlstate,
        message_text,
        mysql_errno,
    })
}

fn parse_quoted_value(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if !s.starts_with('\'') {
        return None;
    }
    let mut escaped = false;
    let mut end = 1;
    let bytes = s.as_bytes();
    while end < bytes.len() {
        if escaped {
            escaped = false;
            end += 1;
            continue;
        }
        if bytes[end] == b'\\' {
            escaped = true;
            end += 1;
            continue;
        }
        if bytes[end] == b'\'' {
            let inner = &s[1..end];
            let unescaped = inner.replace("\\'", "'").replace("\\\\", "\\");
            return Some((unescaped, s.get(end + 1..)?));
        }
        end += 1;
    }
    None
}

fn split_signal_items(s: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut in_quote = false;
    let mut escaped = false;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if b == b'\\' && in_quote {
            escaped = true;
            continue;
        }
        if b == b'\'' {
            in_quote = !in_quote;
            continue;
        }
        if b == b',' && !in_quote {
            items.push(s[start..i].to_string());
            start = i + 1;
        }
    }
    items.push(s[start..].to_string());
    items
}

fn parse_signal_value(val: &str) -> Option<String> {
    if val.starts_with('\'') {
        let (s, _) = parse_quoted_value(val)?;
        Some(s)
    } else {
        Some(val.to_string())
    }
}

fn parse_signal_errno(val: &str) -> Option<u16> {
    val.trim().parse().ok()
}

/// One procedure-local variable from `DECLARE name type [DEFAULT expr]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalVarDecl {
    pub name: String,
    pub data_type: String,
    pub default_value: Option<String>,
}

/// One procedure cursor from `DECLARE name CURSOR FOR select`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalCursorDecl {
    pub name: String,
    pub select_sql: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclareBodyError {
    MisplacedDeclare,
    DuplicateVariable(String),
    InvalidDeclare,
}

/// True for `DECLARE v …` variable declarations (not cursors/handlers — M169/M170).
pub fn is_declare_variable_statement(sql: &str) -> bool {
    let t = sql.trim().trim_end_matches(';').trim();
    let Some(after) = skip_keyword(t, "DECLARE") else {
        return false;
    };
    let Some((_, rest)) = take_ident(after.trim_start()) else {
        return false;
    };
    let rest = rest.trim();
    let upper = rest.to_ascii_uppercase();
    !(upper.starts_with("CURSOR")
        || upper.starts_with("CONTINUE")
        || upper.starts_with("EXIT")
        || upper.starts_with("HANDLER"))
}

/// Parse `DECLARE n INT [DEFAULT 1]` when `is_declare_variable_statement`.
pub fn try_parse_declare_variable(sql: &str) -> Option<LocalVarDecl> {
    if !is_declare_variable_statement(sql) {
        return None;
    }
    let rest = sql
        .trim()
        .trim_end_matches(';')
        .trim()
        .get(7..)?
        .trim_start();
    let (name, rest) = take_ident(rest)?;
    let rest = rest.trim();
    let upper = rest.to_ascii_uppercase();
    let (data_type, default_value) = if let Some(idx) = upper.find(" DEFAULT ") {
        let data_type = rest[..idx].trim().to_string();
        if data_type.is_empty() {
            return None;
        }
        let default_expr = rest[idx + 9..].trim();
        if default_expr.is_empty() {
            return None;
        }
        (data_type, Some(default_expr.to_string()))
    } else {
        let data_type = rest.trim().to_string();
        if data_type.is_empty() {
            return None;
        }
        (data_type, None)
    };
    Some(LocalVarDecl {
        name,
        data_type,
        default_value,
    })
}

/// True for `DECLARE cur CURSOR FOR …` (M169).
pub fn is_declare_cursor_statement(sql: &str) -> bool {
    let t = sql.trim().trim_end_matches(';').trim();
    let Some(after) = skip_keyword(t, "DECLARE") else {
        return false;
    };
    let Some((_, rest)) = take_ident(after.trim_start()) else {
        return false;
    };
    starts_word_keyword(rest.trim_start(), "CURSOR")
}

/// Parse `DECLARE cur CURSOR FOR SELECT …` when [`is_declare_cursor_statement`].
pub fn try_parse_declare_cursor(sql: &str) -> Option<LocalCursorDecl> {
    if !is_declare_cursor_statement(sql) {
        return None;
    }
    let t = sql.trim().trim_end_matches(';').trim();
    let after_declare = skip_keyword(t, "DECLARE")?.trim_start();
    let (name, rest) = take_ident(after_declare)?;
    let rest = skip_keyword(rest.trim_start(), "CURSOR")?.trim_start();
    let rest = skip_keyword(rest, "FOR")?.trim_start();
    let select_sql = rest.trim().to_string();
    if select_sql.is_empty() {
        return None;
    }
    Some(LocalCursorDecl { name, select_sql })
}

/// Leading `DECLARE` variables/cursors and executable statements from a procedure body.
pub type ProcedureBodySplit = (Vec<LocalVarDecl>, Vec<LocalCursorDecl>, Vec<String>);

/// Split a procedure `body` into leading `DECLARE`s and executable statements.
pub fn split_procedure_body(body: &[String]) -> Result<ProcedureBodySplit, DeclareBodyError> {
    let mut declares = Vec::new();
    let mut cursors = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut past_declares = false;
    let mut statements = Vec::new();
    for stmt in body {
        if is_declare_variable_statement(stmt) {
            if past_declares {
                return Err(DeclareBodyError::MisplacedDeclare);
            }
            let decl = try_parse_declare_variable(stmt).ok_or(DeclareBodyError::InvalidDeclare)?;
            let key = decl.name.to_ascii_lowercase();
            if !seen.insert(key.clone()) {
                return Err(DeclareBodyError::DuplicateVariable(decl.name));
            }
            declares.push(decl);
        } else if is_declare_cursor_statement(stmt) {
            if past_declares {
                return Err(DeclareBodyError::MisplacedDeclare);
            }
            let decl = try_parse_declare_cursor(stmt).ok_or(DeclareBodyError::InvalidDeclare)?;
            let key = decl.name.to_ascii_lowercase();
            if !seen.insert(key.clone()) {
                return Err(DeclareBodyError::DuplicateVariable(decl.name));
            }
            cursors.push(decl);
        } else {
            past_declares = true;
            statements.push(stmt.clone());
        }
    }
    Ok((declares, cursors, statements))
}

/// Maximum `WHILE`/`LOOP` iterations per `CALL` (documented; prevents hung CI).
pub const PROCEDURE_MAX_LOOP_ITERATIONS: u32 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcedureBodyStmt {
    Sql(String),
    If {
        condition: String,
        then_stmts: Vec<ProcedureBodyStmt>,
        elseifs: Vec<(String, Vec<ProcedureBodyStmt>)>,
        else_stmts: Option<Vec<ProcedureBodyStmt>>,
    },
    While {
        label: Option<String>,
        condition: String,
        body: Vec<ProcedureBodyStmt>,
    },
    Loop {
        label: Option<String>,
        body: Vec<ProcedureBodyStmt>,
    },
    Leave {
        label: String,
    },
    OpenCursor {
        name: String,
    },
    FetchCursor {
        name: String,
        into_vars: Vec<String>,
    },
    CloseCursor {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcedureControlError {
    InvalidSyntax,
}

/// Parse executable procedure statements (after leading `DECLARE`s).
pub fn parse_procedure_statements(
    stmts: &[String],
) -> Result<Vec<ProcedureBodyStmt>, ProcedureControlError> {
    stmts.iter().map(|s| parse_procedure_body_stmt(s)).collect()
}

fn parse_procedure_body_stmt(sql: &str) -> Result<ProcedureBodyStmt, ProcedureControlError> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return Err(ProcedureControlError::InvalidSyntax);
    }
    let (label, rest) = take_optional_label(trimmed);
    let rest = rest.trim();
    let upper = rest.to_ascii_uppercase();
    if upper.starts_with("LEAVE ") {
        let after = rest.get(6..).unwrap_or("").trim();
        let (lab, tail) = take_ident(after).ok_or(ProcedureControlError::InvalidSyntax)?;
        if !tail.trim().is_empty() {
            return Err(ProcedureControlError::InvalidSyntax);
        }
        return Ok(ProcedureBodyStmt::Leave { label: lab });
    }
    if starts_word_keyword(rest, "IF") {
        return parse_if_body(label, rest);
    }
    if starts_word_keyword(rest, "WHILE") {
        return parse_while_body(label, rest);
    }
    if starts_word_keyword(rest, "LOOP") {
        return parse_loop_body(label, rest);
    }
    if starts_word_keyword(rest, "OPEN") {
        let after = rest.get(4..).unwrap_or("").trim();
        let (name, tail) = take_ident(after).ok_or(ProcedureControlError::InvalidSyntax)?;
        if !tail.trim().is_empty() {
            return Err(ProcedureControlError::InvalidSyntax);
        }
        let _ = label;
        return Ok(ProcedureBodyStmt::OpenCursor { name });
    }
    if starts_word_keyword(rest, "CLOSE") {
        let after = rest.get(5..).unwrap_or("").trim();
        let (name, tail) = take_ident(after).ok_or(ProcedureControlError::InvalidSyntax)?;
        if !tail.trim().is_empty() {
            return Err(ProcedureControlError::InvalidSyntax);
        }
        let _ = label;
        return Ok(ProcedureBodyStmt::CloseCursor { name });
    }
    if starts_word_keyword(rest, "FETCH") {
        let _ = label;
        return parse_fetch_cursor(rest);
    }
    let _ = label;
    Ok(ProcedureBodyStmt::Sql(trimmed.to_string()))
}

fn parse_fetch_cursor(rest: &str) -> Result<ProcedureBodyStmt, ProcedureControlError> {
    let after_fetch = rest.get(5..).unwrap_or("").trim();
    let (name, rest) = take_ident(after_fetch).ok_or(ProcedureControlError::InvalidSyntax)?;
    let rest = rest.trim();
    let upper = rest.to_ascii_uppercase();
    if !upper.starts_with("INTO ") {
        return Err(ProcedureControlError::InvalidSyntax);
    }
    let vars_part = rest.get(5..).unwrap_or("").trim();
    if vars_part.is_empty() {
        return Err(ProcedureControlError::InvalidSyntax);
    }
    let into_vars: Vec<String> = vars_part
        .split(',')
        .map(|v| v.trim().trim_matches('`').to_string())
        .filter(|v| !v.is_empty())
        .collect();
    if into_vars.is_empty() {
        return Err(ProcedureControlError::InvalidSyntax);
    }
    Ok(ProcedureBodyStmt::FetchCursor { name, into_vars })
}

fn take_optional_label(s: &str) -> (Option<String>, &str) {
    let s = s.trim_start();
    let Some((name, rest)) = take_ident(s) else {
        return (None, s);
    };
    let rest = rest.trim_start();
    if let Some(after) = rest.strip_prefix(':') {
        (Some(name), after.trim_start())
    } else {
        (None, s)
    }
}

fn starts_word_keyword(s: &str, keyword: &str) -> bool {
    let s = s.trim_start();
    if s.len() < keyword.len() {
        return false;
    }
    if !s
        .get(..keyword.len())
        .is_some_and(|p| p.eq_ignore_ascii_case(keyword))
    {
        return false;
    }
    matches!(
        s.as_bytes().get(keyword.len()),
        None | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
    )
}

fn keyword_at(s: &str, pos: usize, keyword: &str) -> bool {
    if pos + keyword.len() > s.len() {
        return false;
    }
    if !s.as_bytes()[pos..pos + keyword.len()].eq_ignore_ascii_case(keyword.as_bytes()) {
        return false;
    }
    let before_ok = pos == 0 || !is_ident_byte(s.as_bytes()[pos - 1]);
    let after_pos = pos + keyword.len();
    let after_ok = after_pos >= s.len() || !is_ident_byte(s.as_bytes()[after_pos]);
    before_ok && after_ok
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CompoundFrame {
    If,
    While,
    Loop,
}

/// Split on `;` respecting nested IF/WHILE/LOOP and string literals.
fn split_compound_aware_statements(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut quote: Option<u8> = None;
    let mut stack: Vec<CompoundFrame> = Vec::new();

    while i < len {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == q && (i == 0 || bytes[i - 1] != b'\\') {
                quote = None;
            }
            i += 1;
            continue;
        }
        if b == b'\'' || b == b'"' {
            quote = Some(b);
            i += 1;
            continue;
        }
        if b == b'`' {
            i += 1;
            while i < len && bytes[i] != b'`' {
                i += 1;
            }
            if i < len {
                i += 1;
            }
            continue;
        }
        if keyword_at(src, i, "END IF") {
            if stack.last() == Some(&CompoundFrame::If) {
                stack.pop();
            }
            i += 6;
            continue;
        }
        if keyword_at(src, i, "END WHILE") {
            if stack.last() == Some(&CompoundFrame::While) {
                stack.pop();
            }
            i += 9;
            continue;
        }
        if keyword_at(src, i, "END LOOP") {
            if stack.last() == Some(&CompoundFrame::Loop) {
                stack.pop();
            }
            i += 8;
            continue;
        }
        if keyword_at(src, i, "ELSEIF") {
            i += 6;
            continue;
        }
        if stack.is_empty() {
            if keyword_at(src, i, "IF") {
                stack.push(CompoundFrame::If);
                i += 2;
                continue;
            }
            if keyword_at(src, i, "WHILE") {
                stack.push(CompoundFrame::While);
                i += 5;
                continue;
            }
            if keyword_at(src, i, "LOOP") {
                stack.push(CompoundFrame::Loop);
                i += 4;
                continue;
            }
        } else if keyword_at(src, i, "IF") {
            stack.push(CompoundFrame::If);
            i += 2;
            continue;
        } else if keyword_at(src, i, "WHILE") {
            stack.push(CompoundFrame::While);
            i += 5;
            continue;
        } else if keyword_at(src, i, "LOOP") {
            stack.push(CompoundFrame::Loop);
            i += 4;
            continue;
        }
        if b == b';' && stack.is_empty() {
            let piece = src[start..i].trim();
            if !piece.is_empty() {
                out.push(piece.to_string());
            }
            i += 1;
            start = i;
            continue;
        }
        i += 1;
    }
    let tail = src[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn parse_stmt_list(src: &str) -> Result<Vec<ProcedureBodyStmt>, ProcedureControlError> {
    split_compound_aware_statements(src)
        .into_iter()
        .map(|s| parse_procedure_body_stmt(&s))
        .collect()
}

fn find_then_keyword(s: &str) -> Option<usize> {
    let mut if_depth = 0i32;
    let mut i = 0usize;
    while i < s.len() {
        if keyword_at(s, i, "END IF") {
            if if_depth > 0 {
                if_depth -= 1;
            }
            i += 6;
            continue;
        }
        if keyword_at(s, i, "IF") {
            if_depth += 1;
            i += 2;
            continue;
        }
        if if_depth == 0 && keyword_at(s, i, "THEN") {
            return Some(i);
        }
        i += 1;
    }
    None
}

enum IfBoundary {
    ElseIf,
    Else,
    EndIf,
}

fn scan_if_boundary_at_depth1(s: &str) -> Option<(usize, IfBoundary)> {
    let mut if_depth = 1i32;
    let mut pos = 0usize;
    while pos < s.len() {
        if keyword_at(s, pos, "END IF") {
            if if_depth == 1 {
                return Some((pos, IfBoundary::EndIf));
            }
            if_depth -= 1;
            pos += 6;
            continue;
        }
        if keyword_at(s, pos, "IF") {
            if_depth += 1;
            pos += 2;
            continue;
        }
        if if_depth == 1 {
            if keyword_at(s, pos, "ELSEIF") {
                return Some((pos, IfBoundary::ElseIf));
            }
            if keyword_at(s, pos, "ELSE") {
                return Some((pos, IfBoundary::Else));
            }
        }
        pos += 1;
    }
    None
}

fn parse_if_body(
    _label: Option<String>,
    rest: &str,
) -> Result<ProcedureBodyStmt, ProcedureControlError> {
    let after_if = skip_keyword(rest, "IF").ok_or(ProcedureControlError::InvalidSyntax)?;
    let then_pos = find_then_keyword(after_if).ok_or(ProcedureControlError::InvalidSyntax)?;
    let condition = after_if[..then_pos].trim().to_string();
    if condition.is_empty() {
        return Err(ProcedureControlError::InvalidSyntax);
    }
    let mut tail = after_if[then_pos + 4..].trim_start();
    let mut branch_bodies: Vec<(String, String)> = vec![(condition, String::new())];
    let mut else_body: Option<String> = None;

    loop {
        let (boundary_pos, kind) =
            scan_if_boundary_at_depth1(tail).ok_or(ProcedureControlError::InvalidSyntax)?;
        let segment = tail[..boundary_pos].trim().to_string();
        match kind {
            IfBoundary::EndIf => {
                if else_body.is_none() {
                    branch_bodies.last_mut().unwrap().1 = segment;
                } else {
                    else_body = Some(segment);
                }
                let after = tail[boundary_pos + 6..].trim();
                if !after.is_empty() {
                    return Err(ProcedureControlError::InvalidSyntax);
                }
                break;
            }
            IfBoundary::ElseIf => {
                branch_bodies.last_mut().unwrap().1 = segment;
                tail = tail[boundary_pos + 6..].trim_start();
                let then_pos =
                    find_then_keyword(tail).ok_or(ProcedureControlError::InvalidSyntax)?;
                let cond = tail[..then_pos].trim().to_string();
                if cond.is_empty() {
                    return Err(ProcedureControlError::InvalidSyntax);
                }
                tail = tail[then_pos + 4..].trim_start();
                branch_bodies.push((cond, String::new()));
            }
            IfBoundary::Else => {
                branch_bodies.last_mut().unwrap().1 = segment;
                tail = tail[boundary_pos + 4..].trim_start();
                let (end_pos, end_kind) =
                    scan_if_boundary_at_depth1(tail).ok_or(ProcedureControlError::InvalidSyntax)?;
                if !matches!(end_kind, IfBoundary::EndIf) {
                    return Err(ProcedureControlError::InvalidSyntax);
                }
                else_body = Some(tail[..end_pos].trim().to_string());
                let after = tail[end_pos + 6..].trim();
                if !after.is_empty() {
                    return Err(ProcedureControlError::InvalidSyntax);
                }
                break;
            }
        }
    }

    let (first_cond, first_body) = branch_bodies.remove(0);
    let then_stmts = parse_stmt_list(&first_body)?;
    let mut elseifs = Vec::new();
    for (cond, body) in branch_bodies {
        elseifs.push((cond, parse_stmt_list(&body)?));
    }
    let else_stmts = else_body.map(|b| parse_stmt_list(&b)).transpose()?;
    Ok(ProcedureBodyStmt::If {
        condition: first_cond,
        then_stmts,
        elseifs,
        else_stmts,
    })
}

fn find_while_do(s: &str) -> Option<usize> {
    let mut if_depth = 0i32;
    let mut pos = 0usize;
    while pos < s.len() {
        if keyword_at(s, pos, "END IF") {
            if if_depth > 0 {
                if_depth -= 1;
            }
            pos += 6;
            continue;
        }
        if keyword_at(s, pos, "IF") {
            if_depth += 1;
            pos += 2;
            continue;
        }
        if if_depth == 0 && keyword_at(s, pos, "DO") {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

fn parse_while_body(
    label: Option<String>,
    rest: &str,
) -> Result<ProcedureBodyStmt, ProcedureControlError> {
    let after = skip_keyword(rest, "WHILE").ok_or(ProcedureControlError::InvalidSyntax)?;
    let do_pos = find_while_do(after).ok_or(ProcedureControlError::InvalidSyntax)?;
    let condition = after[..do_pos].trim().to_string();
    if condition.is_empty() {
        return Err(ProcedureControlError::InvalidSyntax);
    }
    let tail = after[do_pos + 2..].trim_start();
    let body_end = scan_while_end(tail).ok_or(ProcedureControlError::InvalidSyntax)?;
    let body_src = tail[..body_end].trim();
    let after_end = tail[body_end + 9..].trim();
    if !after_end.is_empty() {
        return Err(ProcedureControlError::InvalidSyntax);
    }
    Ok(ProcedureBodyStmt::While {
        label,
        condition,
        body: parse_stmt_list(body_src)?,
    })
}

fn parse_loop_body(
    label: Option<String>,
    rest: &str,
) -> Result<ProcedureBodyStmt, ProcedureControlError> {
    let after = skip_keyword(rest, "LOOP").ok_or(ProcedureControlError::InvalidSyntax)?;
    let body_end = scan_loop_end(after).ok_or(ProcedureControlError::InvalidSyntax)?;
    let body_src = after[..body_end].trim();
    let after_end = after[body_end + 8..].trim();
    if !after_end.is_empty() {
        return Err(ProcedureControlError::InvalidSyntax);
    }
    Ok(ProcedureBodyStmt::Loop {
        label,
        body: parse_stmt_list(body_src)?,
    })
}

fn scan_while_end(s: &str) -> Option<usize> {
    let mut while_depth = 1i32;
    let mut pos = 0usize;
    while pos < s.len() {
        if keyword_at(s, pos, "END WHILE") && while_depth == 1 {
            return Some(pos);
        }
        if keyword_at(s, pos, "END WHILE") {
            while_depth -= 1;
            pos += 9;
            continue;
        }
        if keyword_at(s, pos, "WHILE") {
            while_depth += 1;
            pos += 5;
            continue;
        }
        if keyword_at(s, pos, "END LOOP") {
            pos += 8;
            continue;
        }
        if keyword_at(s, pos, "LOOP") {
            pos += 4;
            continue;
        }
        if keyword_at(s, pos, "END IF") {
            pos += 6;
            continue;
        }
        if keyword_at(s, pos, "IF") {
            pos += 2;
            continue;
        }
        pos += 1;
    }
    None
}

fn scan_loop_end(s: &str) -> Option<usize> {
    let mut loop_depth = 1i32;
    let mut pos = 0usize;
    while pos < s.len() {
        if keyword_at(s, pos, "END LOOP") && loop_depth == 1 {
            return Some(pos);
        }
        if keyword_at(s, pos, "END LOOP") {
            loop_depth -= 1;
            pos += 8;
            continue;
        }
        if keyword_at(s, pos, "LOOP") {
            loop_depth += 1;
            pos += 4;
            continue;
        }
        if keyword_at(s, pos, "END WHILE") {
            pos += 9;
            continue;
        }
        if keyword_at(s, pos, "WHILE") {
            pos += 5;
            continue;
        }
        if keyword_at(s, pos, "END IF") {
            pos += 6;
            continue;
        }
        if keyword_at(s, pos, "IF") {
            pos += 2;
            continue;
        }
        pos += 1;
    }
    None
}

fn extract_procedure_body(full: &str, after_params: &str) -> Option<Vec<String>> {
    let trimmed = after_params.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.to_ascii_uppercase().starts_with("BEGIN") {
        return extract_body(full);
    }
    let stmt = trimmed.trim_end_matches(';').trim();
    if stmt.is_empty() {
        return None;
    }
    Some(vec![stmt.to_string()])
}

fn extract_body(input: &str) -> Option<Vec<String>> {
    let begin = input.to_ascii_uppercase().find("BEGIN")?;
    let end = input.to_ascii_uppercase().rfind("END")?;
    let inner = input[begin + 5..end].trim();
    Some(split_compound_aware_statements(inner))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_control_parse_if_while_loop() {
        let body = try_parse_stored_program(
            "CREATE PROCEDURE p() BEGIN IF 1>0 THEN SELECT 1; ELSE SELECT 0; END IF; END",
        )
        .unwrap();
        let StoredProgramStmt::CreateProcedure(meta) = body else {
            panic!("expected procedure");
        };
        assert_eq!(meta.body.len(), 1);
        let stmts = parse_procedure_statements(&meta.body).expect("parse");
        assert!(matches!(stmts[0], ProcedureBodyStmt::If { .. }));

        let while_body = try_parse_stored_program(
            "CREATE PROCEDURE w() BEGIN DECLARE n INT DEFAULT 2; WHILE n > 0 DO SET n = n - 1; END WHILE; SELECT n; END",
        )
        .unwrap();
        let StoredProgramStmt::CreateProcedure(wmeta) = while_body else {
            panic!("expected procedure");
        };
        let (declares, _cursors, exec) = split_procedure_body(&wmeta.body).expect("split");
        assert_eq!(declares.len(), 1);
        let parsed = parse_procedure_statements(&exec).expect("while parse");
        assert!(matches!(parsed[0], ProcedureBodyStmt::While { .. }));

        let loop_body = try_parse_stored_program(
            "CREATE PROCEDURE l() BEGIN DECLARE n INT DEFAULT 0; lbl: LOOP SET n = n + 1; IF n >= 3 THEN LEAVE lbl; END IF; END LOOP; SELECT n; END",
        )
        .unwrap();
        let StoredProgramStmt::CreateProcedure(lmeta) = loop_body else {
            panic!("expected procedure");
        };
        let (_, _cursors, exec) = split_procedure_body(&lmeta.body).expect("split loop");
        assert_eq!(exec.len(), 2);
        let parsed = parse_procedure_statements(&exec).expect("loop parse");
        assert!(matches!(parsed[0], ProcedureBodyStmt::Loop { .. }));
    }

    #[test]
    fn declare_var_parse_and_body_split() {
        let decl = try_parse_declare_variable("DECLARE n INT DEFAULT 1").expect("parse declare");
        assert_eq!(decl.name, "n");
        assert_eq!(decl.data_type, "INT");
        assert_eq!(decl.default_value.as_deref(), Some("1"));

        let no_def = try_parse_declare_variable("DECLARE cnt INT").expect("no default");
        assert!(no_def.default_value.is_none());

        assert!(!is_declare_variable_statement(
            "DECLARE cur CURSOR FOR SELECT 1"
        ));
        assert!(!is_declare_variable_statement(
            "DECLARE CONTINUE HANDLER FOR SQLEXCEPTION"
        ));

        let body = vec![
            "DECLARE n INT DEFAULT 1".to_string(),
            "SET n = n + 1".to_string(),
            "SELECT n".to_string(),
        ];
        let (declares, cursors, stmts) = split_procedure_body(&body).expect("split");
        assert_eq!(declares.len(), 1);
        assert_eq!(cursors.len(), 0);
        assert_eq!(stmts.len(), 2);

        let bad = vec!["SET n = 1".to_string(), "DECLARE n INT".to_string()];
        assert_eq!(
            split_procedure_body(&bad),
            Err(DeclareBodyError::MisplacedDeclare)
        );

        let dup = vec!["DECLARE n INT".to_string(), "DECLARE n INT".to_string()];
        assert!(matches!(
            split_procedure_body(&dup),
            Err(DeclareBodyError::DuplicateVariable(_))
        ));
    }

    #[test]
    fn cursor_decl_parse_and_open_fetch_close() {
        let cur = try_parse_declare_cursor("DECLARE cur CURSOR FOR SELECT id FROM t ORDER BY id")
            .expect("cursor declare");
        assert_eq!(cur.name, "cur");
        assert_eq!(cur.select_sql, "SELECT id FROM t ORDER BY id");

        let body = vec![
            "DECLARE n INT".to_string(),
            "DECLARE cur CURSOR FOR SELECT 1".to_string(),
            "OPEN cur".to_string(),
            "FETCH cur INTO n".to_string(),
            "CLOSE cur".to_string(),
        ];
        let (declares, cursors, stmts) = split_procedure_body(&body).expect("split cursor body");
        assert_eq!(declares.len(), 1);
        assert_eq!(cursors.len(), 1);
        assert_eq!(stmts.len(), 3);

        let parsed = parse_procedure_statements(&stmts).expect("parse cursor stmts");
        assert!(matches!(parsed[0], ProcedureBodyStmt::OpenCursor { .. }));
        assert!(matches!(parsed[1], ProcedureBodyStmt::FetchCursor { .. }));
        assert!(matches!(parsed[2], ProcedureBodyStmt::CloseCursor { .. }));
    }

    #[test]
    fn parse_signal_and_resignal_statements() {
        let sig = try_parse_signal(
            "SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'nope', MYSQL_ERRNO = 1644",
        )
        .expect("signal parse");
        assert_eq!(sig.sqlstate, "45000");
        assert_eq!(sig.message_text.as_deref(), Some("nope"));
        assert_eq!(sig.mysql_errno, Some(1644));

        let default = try_parse_signal("SIGNAL SQLSTATE '45000'").expect("default signal");
        assert_eq!(default.sqlstate, "45000");
        assert!(default.mysql_errno.is_none());
        assert!(default.message_text.is_none());

        assert!(is_resignal_statement("RESIGNAL"));
        assert!(is_resignal_statement("RESIGNAL SET MESSAGE_TEXT = 'x'"));
        assert!(try_parse_signal("SELECT 1").is_none());
    }

    #[test]
    fn parse_procedure_in_param_and_call_args() {
        let stmt = try_parse_stored_program("CREATE PROCEDURE gap_p(IN x INT) BEGIN SELECT x; END")
            .unwrap();
        let StoredProgramStmt::CreateProcedure(meta) = stmt else {
            panic!("expected create procedure");
        };
        assert_eq!(meta.name, "gap_p");
        assert_eq!(meta.body, vec!["SELECT x".to_string()]);
        assert_eq!(meta.parameters.len(), 1);
        assert_eq!(meta.parameters[0].name, "x");
        assert_eq!(meta.parameters[0].mode, "IN");
        assert_eq!(meta.parameters[0].data_type, "INT");
        assert_eq!(meta.parameters[0].ordinal_position, 1);

        let implicit = try_parse_stored_program("CREATE PROCEDURE p(x INT) SELECT x AS n").unwrap();
        let StoredProgramStmt::CreateProcedure(meta) = implicit else {
            panic!("expected create procedure");
        };
        assert_eq!(meta.body, vec!["SELECT x AS n".to_string()]);
        assert_eq!(meta.parameters[0].mode, "IN");
        assert_eq!(meta.parameters[0].name, "x");

        let zero =
            try_parse_stored_program("CREATE PROCEDURE p() BEGIN INSERT INTO t VALUES (42); END")
                .unwrap();
        let StoredProgramStmt::CreateProcedure(meta) = zero else {
            panic!("expected create procedure");
        };
        assert!(meta.parameters.is_empty());
        assert_eq!(meta.body, vec!["INSERT INTO t VALUES (42)".to_string()]);

        let call = try_parse_stored_program("CALL gap_p(3)").unwrap();
        let StoredProgramStmt::Call { name, args, .. } = call else {
            panic!("expected call");
        };
        assert_eq!(name, "gap_p");
        assert_eq!(args, vec!["3".to_string()]);

        let empty_call = try_parse_stored_program("CALL p()").unwrap();
        let StoredProgramStmt::Call { args, .. } = empty_call else {
            panic!("expected call");
        };
        assert!(args.is_empty());

        let out_stmt =
            try_parse_stored_program("CREATE PROCEDURE p(OUT x INT) BEGIN SELECT x; END").unwrap();
        let StoredProgramStmt::CreateProcedure(out_meta) = out_stmt else {
            panic!("expected create procedure with OUT");
        };
        assert_eq!(out_meta.parameters[0].mode, "OUT");

        let inout_stmt =
            try_parse_stored_program("CREATE PROCEDURE p(INOUT x INT) BEGIN SELECT x; END")
                .unwrap();
        let StoredProgramStmt::CreateProcedure(inout_meta) = inout_stmt else {
            panic!("expected create procedure with INOUT");
        };
        assert_eq!(inout_meta.parameters[0].mode, "INOUT");
    }

    #[test]
    fn procedure_out_inout_create_and_call_parse() {
        let stmt =
            try_parse_stored_program("CREATE PROCEDURE gap_out(OUT x INT) BEGIN SET x = 3; END")
                .unwrap();
        let StoredProgramStmt::CreateProcedure(meta) = stmt else {
            panic!("expected create procedure");
        };
        assert_eq!(meta.parameters[0].mode, "OUT");
        assert_eq!(meta.parameters[0].name, "x");

        let call = try_parse_stored_program("CALL gap_out(@a)").unwrap();
        let StoredProgramStmt::Call { name, args, .. } = call else {
            panic!("expected call");
        };
        assert_eq!(name, "gap_out");
        assert_eq!(args, vec!["@a".to_string()]);
    }

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
        assert!(!meta.disable_on_slave);
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
        assert!(!meta.disable_on_slave);
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
        let StoredProgramStmt::AlterEvent {
            status,
            disable_on_slave,
            ..
        } = stmt
        else {
            panic!("expected alter event");
        };
        assert_eq!(status.as_deref(), Some("DISABLED"));
        assert_eq!(disable_on_slave, Some(false));

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

    #[test]
    fn parse_create_event_disable_on_slave() {
        let stmt = try_parse_stored_program(
            "CREATE EVENT e_dos ON SCHEDULE EVERY 1 HOUR DISABLE ON SLAVE DO SELECT 1",
        )
        .unwrap();
        let StoredProgramStmt::CreateEvent { meta, .. } = stmt else {
            panic!("expected create event");
        };
        assert_eq!(meta.name, "e_dos");
        assert_eq!(meta.status, "ENABLED");
        assert!(meta.disable_on_slave);
        assert_eq!(meta.interval_field.as_deref(), Some("HOUR"));
        assert_eq!(meta.body, "SELECT 1");

        let stmt = try_parse_stored_program(
            "CREATE EVENT e ON SCHEDULE EVERY 1 HOUR ENABLE ON SLAVE DO SELECT 1",
        )
        .unwrap();
        let StoredProgramStmt::CreateEvent { meta, .. } = stmt else {
            panic!("expected create event");
        };
        assert_eq!(meta.status, "ENABLED");
        assert!(!meta.disable_on_slave);

        let stmt = try_parse_stored_program("ALTER EVENT e DISABLE ON SLAVE").unwrap();
        let StoredProgramStmt::AlterEvent {
            status,
            disable_on_slave,
            ..
        } = stmt
        else {
            panic!("expected alter event");
        };
        assert_eq!(status.as_deref(), Some("ENABLED"));
        assert_eq!(disable_on_slave, Some(true));

        let stmt = try_parse_stored_program("ALTER EVENT e ENABLE").unwrap();
        let StoredProgramStmt::AlterEvent {
            status,
            disable_on_slave,
            ..
        } = stmt
        else {
            panic!("expected alter event");
        };
        assert_eq!(status.as_deref(), Some("ENABLED"));
        assert_eq!(disable_on_slave, Some(false));
    }
}

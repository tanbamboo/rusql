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
    if u.starts_with("CREATE EVENT") {
        return parse_create_event(t);
    }
    if u.starts_with("DROP EVENT") {
        return parse_drop_event(t);
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

fn parse_create_event(input: &str) -> Option<StoredProgramStmt> {
    let rest = skip_keyword(input, "CREATE")?;
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
    let (schedule_type, execute_at, interval_value, interval_field, rest) =
        if let Some(after_at) = skip_keyword(rest, "AT") {
            let (ts, rest) = take_quoted_string(after_at)?;
            ("ONE TIME".to_string(), Some(ts), None, None, rest)
        } else {
            let after_every = skip_keyword(rest, "EVERY")?;
            let (value, rest) = take_number(after_every)?;
            let (unit, rest) = take_ident(rest)?;
            let unit = unit.to_ascii_uppercase();
            if !INTERVAL_UNITS.iter().any(|u| *u == unit) {
                return None;
            }
            ("RECURRING".to_string(), None, Some(value), Some(unit), rest)
        };
    let (status, rest) = if let Some(after) = skip_keyword(rest, "ENABLE") {
        ("ENABLED".to_string(), after)
    } else if let Some(after) = skip_keyword(rest, "DISABLE") {
        ("DISABLED".to_string(), after)
    } else {
        ("ENABLED".to_string(), rest)
    };
    let rest = skip_keyword(rest, "DO")?;
    let body = rest.trim().trim_end_matches(';').trim();
    if body.is_empty() {
        return None;
    }
    Some(StoredProgramStmt::CreateEvent {
        meta: EventMeta {
            schema,
            name,
            schedule_type,
            execute_at,
            interval_value,
            interval_field,
            status,
            body: body.to_string(),
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
}

//! Parse CREATE PROCEDURE / CALL / CREATE TRIGGER / CREATE FUNCTION / DROP (MVP).
use rusql_core::{
    FunctionMeta, ProcedureMeta, TriggerEvent, TriggerMeta, TriggerTiming, DEFAULT_SCHEMA,
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
    let return_type = input[returns_pos + 9..begin_pos].trim().to_string();
    let body = extract_body(input)?;
    let return_expr = extract_return_expr(&body)?;
    Some(StoredProgramStmt::CreateFunction(FunctionMeta {
        schema,
        name,
        return_type,
        return_expr,
    }))
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
    }
}

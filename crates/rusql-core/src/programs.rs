//! Stored procedures and triggers metadata (MVP).
use crate::Catalog;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TriggerTiming {
    Before,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TriggerEvent {
    Insert,
    Update,
    Delete,
}

/// One stored-program parameter persisted on the catalog (M121 / M132).
/// M132 records `IN` lists on `CREATE PROCEDURE`. `OUT`/`INOUT` stay M165.
/// `CREATE FUNCTION` still does not record parameter lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ParameterMeta {
    pub name: String,
    #[serde(default)]
    pub mode: String,
    pub data_type: String,
    pub ordinal_position: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionMeta {
    pub schema: String,
    pub name: String,
    pub return_type: String,
    pub return_expr: String,
}

impl FunctionMeta {
    /// Catalogued parameters as `(ordinal, mode, name, data_type)`.
    /// Empty until M132 persists `IN` lists. Do not invent return-value rows.
    pub fn parameters(&self) -> impl Iterator<Item = (u32, &str, &str, &str)> {
        let stored: &[ParameterMeta] = &[];
        stored.iter().map(|p| {
            (
                p.ordinal_position,
                p.mode.as_str(),
                p.name.as_str(),
                p.data_type.as_str(),
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureMeta {
    pub schema: String,
    pub name: String,
    pub body: Vec<String>,
    /// `IN` parameters from `CREATE PROCEDURE` (M132). Missing `programs.json`
    /// fields stay empty — do not invent rows for old catalog entries.
    #[serde(default)]
    pub parameters: Vec<ParameterMeta>,
}

impl ProcedureMeta {
    /// Catalogued parameters as `(ordinal, mode, name, data_type)`.
    /// Empty when the procedure was created with `()` or loaded from old JSON.
    pub fn parameters(&self) -> impl Iterator<Item = (u32, &str, &str, &str)> {
        self.parameters.iter().map(|p| {
            let mode = if p.mode.is_empty() {
                "IN"
            } else {
                p.mode.as_str()
            };
            (
                p.ordinal_position,
                mode,
                p.name.as_str(),
                p.data_type.as_str(),
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerMeta {
    pub schema: String,
    pub table: String,
    pub name: String,
    pub timing: TriggerTiming,
    pub event: TriggerEvent,
    pub body: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMeta {
    pub schema: String,
    pub name: String,
    /// MySQL `SHOW EVENTS` `Type`: `ONE TIME` or `RECURRING`.
    pub schedule_type: String,
    pub execute_at: Option<String>,
    pub interval_value: Option<String>,
    pub interval_field: Option<String>,
    /// `ENABLED` or `DISABLED`.
    pub status: String,
    /// Statement after `DO` (not executed by a scheduler).
    pub body: String,
    /// Internal scheduler watermark (`YYYY-MM-DD HH:MM:SS`); not a `SHOW EVENTS` column (M105).
    #[serde(default)]
    pub last_executed: Option<String>,
    /// Inclusive window start for `EVERY` (M106). Empty/`None` means no start bound.
    #[serde(default)]
    pub starts: Option<String>,
    /// Inclusive window end for `EVERY` (M106). Empty/`None` means no end bound.
    #[serde(default)]
    pub ends: Option<String>,
    /// `user@host` for `SHOW EVENTS` Definer / `SHOW CREATE EVENT` (M107).
    #[serde(default)]
    pub definer: Option<String>,
    /// `PRESERVE` or `NOT PRESERVE` (M107). `None` means `NOT PRESERVE`.
    #[serde(default)]
    pub on_completion: Option<String>,
    /// Event COMMENT text (M108). Empty/`None` omitted from SHOW CREATE EVENT.
    #[serde(default)]
    pub comment: Option<String>,
    /// MySQL `DISABLE ON SLAVE` (M130). Additive; missing `programs.json` fields stay false.
    /// rusql has no replica role yet — the scheduler treats this as DISABLED.
    #[serde(default)]
    pub disable_on_slave: bool,
}

impl EventMeta {
    /// Scheduler may run this event (ENABLED and not `DISABLE ON SLAVE`).
    pub fn scheduler_enabled(&self) -> bool {
        self.status.eq_ignore_ascii_case("ENABLED") && !self.disable_on_slave
    }

    /// `SHOW EVENTS` / `information_schema.EVENTS` Status cell.
    /// Slave-disabled events use MySQL's `SLAVESIDE_DISABLED`.
    pub fn display_status(&self) -> &str {
        if self.disable_on_slave {
            "SLAVESIDE_DISABLED"
        } else {
            self.status.as_str()
        }
    }
}

pub fn program_key(schema: &str, name: &str) -> String {
    format!("{schema}.{name}")
}
pub fn trigger_key(schema: &str, table: &str, name: &str) -> String {
    format!("{schema}.{table}.{name}")
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ProgramStore {
    pub procedures: HashMap<String, ProcedureMeta>,
    pub triggers: HashMap<String, TriggerMeta>,
    #[serde(default)]
    pub functions: HashMap<String, FunctionMeta>,
    #[serde(default)]
    pub events: HashMap<String, EventMeta>,
}

impl ProgramStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn load(data_dir: &Path) -> Result<Self, String> {
        let path = data_dir.join("programs.json");
        if !path.exists() {
            return Ok(Self::new());
        }
        let bytes = fs::read(&path).map_err(|e| format!("read programs.json: {e}"))?;
        if bytes.is_empty() {
            return Ok(Self::new());
        }
        serde_json::from_slice(&bytes).map_err(|e| format!("parse programs.json: {e}"))
    }
    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| format!("serialize: {e}"))?;
        fs::write(data_dir.join("programs.json"), json).map_err(|e| format!("write: {e}"))
    }
    pub fn seed_catalog(&self, catalog: &mut Catalog) {
        for p in self.procedures.values() {
            catalog.create_procedure(p.clone());
        }
        for f in self.functions.values() {
            catalog.create_function(f.clone());
        }
        for t in self.triggers.values() {
            catalog.create_trigger(t.clone());
        }
        for e in self.events.values() {
            catalog.create_event(e.clone());
        }
    }
    pub fn create_procedure(&mut self, meta: ProcedureMeta) -> Result<(), String> {
        let key = program_key(&meta.schema, &meta.name);
        if self.procedures.contains_key(&key) {
            return Err(rusql_i18n::messages::procedure_exists(&meta.name));
        }
        self.procedures.insert(key, meta);
        Ok(())
    }
    pub fn drop_procedure(&mut self, schema: &str, name: &str) -> Result<(), String> {
        if self.procedures.remove(&program_key(schema, name)).is_none() {
            return Err(rusql_i18n::messages::procedure_not_found(name));
        }
        Ok(())
    }
    pub fn get_procedure(&self, schema: &str, name: &str) -> Option<&ProcedureMeta> {
        self.procedures.get(&program_key(schema, name))
    }
    pub fn create_function(&mut self, meta: FunctionMeta) -> Result<(), String> {
        let key = program_key(&meta.schema, &meta.name);
        if self.functions.contains_key(&key) {
            return Err(rusql_i18n::messages::function_exists(&meta.name));
        }
        self.functions.insert(key, meta);
        Ok(())
    }
    pub fn drop_function(&mut self, schema: &str, name: &str) -> Result<(), String> {
        if self.functions.remove(&program_key(schema, name)).is_none() {
            return Err(rusql_i18n::messages::function_not_found(name));
        }
        Ok(())
    }
    pub fn get_function(&self, schema: &str, name: &str) -> Option<&FunctionMeta> {
        self.functions.get(&program_key(schema, name))
    }
    pub fn create_trigger(&mut self, meta: TriggerMeta) -> Result<(), String> {
        let key = trigger_key(&meta.schema, &meta.table, &meta.name);
        if self.triggers.contains_key(&key) {
            return Err(rusql_i18n::messages::trigger_exists(&meta.name));
        }
        self.triggers.insert(key, meta);
        Ok(())
    }
    pub fn create_event(&mut self, meta: EventMeta) -> Result<(), String> {
        let key = program_key(&meta.schema, &meta.name);
        if self.events.contains_key(&key) {
            return Err(rusql_i18n::messages::event_exists(&meta.name));
        }
        self.events.insert(key, meta);
        Ok(())
    }
    pub fn drop_event(&mut self, schema: &str, name: &str) -> Result<(), String> {
        let key = self
            .events
            .iter()
            .find(|(_, e)| e.schema == schema && e.name.eq_ignore_ascii_case(name))
            .map(|(k, _)| k.clone());
        let Some(key) = key else {
            return Err(rusql_i18n::messages::event_not_found(name));
        };
        self.events.remove(&key);
        Ok(())
    }
    pub fn get_event(&self, schema: &str, name: &str) -> Option<&EventMeta> {
        self.events.get(&program_key(schema, name)).or_else(|| {
            self.events
                .values()
                .find(|e| e.schema == schema && e.name.eq_ignore_ascii_case(name))
        })
    }
    pub fn put_event(&mut self, meta: EventMeta) {
        let key = program_key(&meta.schema, &meta.name);
        self.events.insert(key, meta);
    }
    pub fn drop_trigger_by_name(&mut self, schema: &str, name: &str) -> Result<(), String> {
        let key = self
            .triggers
            .iter()
            .find(|(_, t)| t.schema == schema && t.name.eq_ignore_ascii_case(name))
            .map(|(k, _)| k.clone());
        let Some(key) = key else {
            return Err(rusql_i18n::messages::trigger_not_found(name));
        };
        self.triggers.remove(&key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_json_without_events_still_loads() {
        let store: ProgramStore =
            serde_json::from_str(r#"{"procedures":{},"triggers":{},"functions":{}}"#).unwrap();
        assert!(store.events.is_empty());
    }

    #[test]
    fn event_meta_without_last_executed_still_loads() {
        let store: ProgramStore = serde_json::from_str(
            r#"{"procedures":{},"triggers":{},"functions":{},"events":{"rusql.e":{"schema":"rusql","name":"e","schedule_type":"RECURRING","interval_value":"1","interval_field":"HOUR","status":"ENABLED","body":"SELECT 1"}}}"#,
        )
        .unwrap();
        let meta = store.get_event("rusql", "e").unwrap();
        assert!(meta.last_executed.is_none());
        assert!(meta.starts.is_none());
        assert!(meta.ends.is_none());
        assert!(meta.definer.is_none());
        assert!(meta.on_completion.is_none());
        assert!(meta.comment.is_none());
        assert!(!meta.disable_on_slave);
        assert_eq!(meta.interval_field.as_deref(), Some("HOUR"));
    }

    #[test]
    fn event_meta_disable_on_slave_defaults_false() {
        let store: ProgramStore = serde_json::from_str(
            r#"{"procedures":{},"triggers":{},"functions":{},"events":{"rusql.e":{"schema":"rusql","name":"e","schedule_type":"RECURRING","interval_value":"1","interval_field":"HOUR","status":"ENABLED","body":"SELECT 1"}}}"#,
        )
        .unwrap();
        let meta = store.get_event("rusql", "e").unwrap();
        assert!(!meta.disable_on_slave);
        assert!(meta.scheduler_enabled());
        assert_eq!(meta.display_status(), "ENABLED");

        let mut disabled = meta.clone();
        disabled.disable_on_slave = true;
        assert!(!disabled.scheduler_enabled());
        assert_eq!(disabled.display_status(), "SLAVESIDE_DISABLED");
    }

    #[test]
    fn procedure_and_function_parameters_empty_until_m132() {
        let proc = ProcedureMeta {
            schema: "rusql".into(),
            name: "p".into(),
            body: vec!["SELECT 1".into()],
            parameters: Vec::new(),
        };
        let func = FunctionMeta {
            schema: "rusql".into(),
            name: "f".into(),
            return_type: "INT".into(),
            return_expr: "1".into(),
        };
        assert_eq!(proc.parameters().count(), 0);
        assert_eq!(func.parameters().count(), 0);
        let stored = ParameterMeta {
            name: "x".into(),
            mode: "IN".into(),
            data_type: "INT".into(),
            ordinal_position: 1,
        };
        assert_eq!(stored.name, "x");
        assert_eq!(stored.mode, "IN");
        assert_eq!(stored.data_type, "INT");
        assert_eq!(stored.ordinal_position, 1);
    }

    #[test]
    fn procedure_in_parameters_round_trip_and_old_json_defaults_empty() {
        let proc = ProcedureMeta {
            schema: "rusql".into(),
            name: "gap_p".into(),
            body: vec!["SELECT x".into()],
            parameters: vec![ParameterMeta {
                name: "x".into(),
                mode: "IN".into(),
                data_type: "INT".into(),
                ordinal_position: 1,
            }],
        };
        let rows: Vec<_> = proc.parameters().collect();
        assert_eq!(rows, vec![(1, "IN", "x", "INT")]);

        let store: ProgramStore = serde_json::from_str(
            r#"{"procedures":{"rusql.p":{"schema":"rusql","name":"p","body":["SELECT 1"]}},"triggers":{},"functions":{}}"#,
        )
        .unwrap();
        let old = store.get_procedure("rusql", "p").unwrap();
        assert!(old.parameters.is_empty());
        assert_eq!(old.parameters().count(), 0);
    }
}

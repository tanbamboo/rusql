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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionMeta {
    pub schema: String,
    pub name: String,
    pub return_type: String,
    pub return_expr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureMeta {
    pub schema: String,
    pub name: String,
    pub body: Vec<String>,
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

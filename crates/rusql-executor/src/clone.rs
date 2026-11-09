//! `CLONE LOCAL DATA DIRECTORY` execution (M200).

use std::path::Path;

use crate::{ExecError, QueryResult};
use rusql_sql::CloneStatement;
use rusql_storage::{CloneError, PersistentEngine};

/// Execute a parsed local clone statement against the open engine's data directory.
pub fn execute_clone(
    engine: &PersistentEngine,
    stmt: CloneStatement,
) -> Result<QueryResult, ExecError> {
    match stmt {
        CloneStatement::LocalDataDirectory { path } => {
            engine
                .clone_instance_to(Path::new(&path))
                .map_err(map_clone)?;
            Ok(QueryResult::Ok { rows_affected: 0 })
        }
    }
}

fn map_clone(err: CloneError) -> ExecError {
    match err {
        CloneError::Mysql { code, message } => ExecError::Mysql { code, message },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusql_core::{ColumnDef, TableMeta};
    use rusql_sql::try_parse_clone;
    use rusql_storage::StorageEngine;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rusql-exec-clone-{}-{}-{}",
            std::process::id(),
            label,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn clone_local_data_directory_round_trip() {
        let source = temp_dir("src");
        let dest_parent = temp_dir("dest-parent");
        let dest = dest_parent.join("replica");
        let eng = {
            let mut eng = PersistentEngine::open(&source).unwrap();
            eng.create_table(TableMeta {
                name: "t".into(),
                schema: "rusql".into(),
                columns: vec![ColumnDef::new("id", "INT")],
                auto_increment_next: None,
                ..Default::default()
            })
            .unwrap();
            eng.insert("t", vec!["42".into()]).unwrap();
            eng
        };
        let stmt = try_parse_clone(&format!(
            "CLONE LOCAL DATA DIRECTORY = '{}'",
            dest.display().to_string().replace('\\', "/")
        ))
        .unwrap()
        .unwrap();
        execute_clone(&eng, stmt).unwrap();
        let cloned = PersistentEngine::open(&dest).unwrap();
        assert_eq!(cloned.scan("t").unwrap(), vec![vec!["42".to_string()]]);
        let _ = std::fs::remove_dir_all(&source);
        let _ = std::fs::remove_dir_all(&dest_parent);
    }
}

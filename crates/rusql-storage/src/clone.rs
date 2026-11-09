//! File-level data-directory clone / snapshot (M200).
//!
//! Copies `{source_data_dir}` into an empty destination path for replica
//! provisioning. Not the Oracle clone plugin and not a row in `mysql.plugin`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::StorageError;

/// `ER_CLONE_OS` — operating-system / I/O failure during clone.
pub const ER_CLONE_OS: u16 = 3865;
/// `ER_CLONE_SYS_CONFIG` — destination path invalid or not empty.
pub const ER_CLONE_SYS_CONFIG: u16 = 3869;
/// `ER_CLONE_DISALLOWED` — remote / unsupported clone form rejected.
pub const ER_CLONE_DISALLOWED: u16 = 3875;

/// Client-visible clone failure with a MySQL errno.
#[derive(Debug, thiserror::Error)]
pub enum CloneError {
    /// Errno plus i18n message (`HY000`).
    #[error("{message}")]
    Mysql { code: u16, message: String },
}

impl From<StorageError> for CloneError {
    fn from(err: StorageError) -> Self {
        Self::Mysql {
            code: ER_CLONE_OS,
            message: err.to_string(),
        }
    }
}

fn mysql_err(code: u16, message: impl Into<String>) -> CloneError {
    CloneError::Mysql {
        code,
        message: message.into(),
    }
}

/// Copy `source_dir` into `dest_dir` (must be absent or empty).
///
/// Does **not** acquire a global lock. Concurrent writers may produce a torn
/// snapshot; quiesce the donor for a consistent replica seed. Only files under
/// `source_dir` are copied (no extra secrets outside the data directory).
pub fn clone_instance(source_dir: &Path, dest_dir: &Path) -> Result<(), CloneError> {
    if !source_dir.exists() {
        return Err(mysql_err(
            ER_CLONE_SYS_CONFIG,
            rusql_i18n::messages::sql_clone_source_missing(),
        ));
    }
    if !source_dir.is_dir() {
        return Err(mysql_err(
            ER_CLONE_SYS_CONFIG,
            rusql_i18n::messages::sql_clone_source_not_dir(),
        ));
    }

    let source = normalize(source_dir)?;
    let dest = normalize(dest_dir)?;

    if source == dest {
        return Err(mysql_err(
            ER_CLONE_SYS_CONFIG,
            rusql_i18n::messages::sql_clone_dest_same_as_source(),
        ));
    }
    if is_under(&dest, &source) {
        return Err(mysql_err(
            ER_CLONE_SYS_CONFIG,
            rusql_i18n::messages::sql_clone_dest_inside_source(),
        ));
    }
    if is_under(&source, &dest) {
        return Err(mysql_err(
            ER_CLONE_SYS_CONFIG,
            rusql_i18n::messages::sql_clone_source_inside_dest(),
        ));
    }

    prepare_empty_dest(&dest)?;
    copy_dir_recursive(&source, &dest)?;
    Ok(())
}

fn normalize(path: &Path) -> Result<PathBuf, CloneError> {
    // Prefer absolute paths so nested-path checks are stable even when dirs
    // do not exist yet (destination may be created after this call).
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|e| {
        mysql_err(
            ER_CLONE_OS,
            rusql_i18n::messages::sql_clone_os_error(&e.to_string()),
        )
    })?;
    Ok(cwd.join(path))
}

fn is_under(child: &Path, parent: &Path) -> bool {
    child.starts_with(parent)
}

fn prepare_empty_dest(dest: &Path) -> Result<(), CloneError> {
    if !dest.exists() {
        fs::create_dir_all(dest).map_err(|e| {
            mysql_err(
                ER_CLONE_OS,
                rusql_i18n::messages::sql_clone_os_error(&e.to_string()),
            )
        })?;
        return Ok(());
    }
    if !dest.is_dir() {
        return Err(mysql_err(
            ER_CLONE_SYS_CONFIG,
            rusql_i18n::messages::sql_clone_dest_not_dir(),
        ));
    }
    let mut entries = fs::read_dir(dest).map_err(|e| {
        mysql_err(
            ER_CLONE_OS,
            rusql_i18n::messages::sql_clone_os_error(&e.to_string()),
        )
    })?;
    if entries.next().is_some() {
        return Err(mysql_err(
            ER_CLONE_SYS_CONFIG,
            rusql_i18n::messages::sql_clone_dest_not_empty(),
        ));
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, dest: &Path) -> Result<(), CloneError> {
    let entries = fs::read_dir(source).map_err(|e| {
        mysql_err(
            ER_CLONE_OS,
            rusql_i18n::messages::sql_clone_os_error(&e.to_string()),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            mysql_err(
                ER_CLONE_OS,
                rusql_i18n::messages::sql_clone_os_error(&e.to_string()),
            )
        })?;
        let file_type = entry.file_type().map_err(|e| {
            mysql_err(
                ER_CLONE_OS,
                rusql_i18n::messages::sql_clone_os_error(&e.to_string()),
            )
        })?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if file_type.is_dir() {
            fs::create_dir_all(&to).map_err(|e| {
                mysql_err(
                    ER_CLONE_OS,
                    rusql_i18n::messages::sql_clone_os_error(&e.to_string()),
                )
            })?;
            copy_dir_recursive(&from, &to)?;
        } else if file_type.is_file() {
            fs::copy(&from, &to).map_err(|e| {
                mysql_err(
                    ER_CLONE_OS,
                    rusql_i18n::messages::sql_clone_os_error(&e.to_string()),
                )
            })?;
        }
        // Skip symlinks and other special files (do not follow out of data_dir).
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PersistentEngine, StorageEngine};
    use rusql_core::{ColumnDef, TableMeta};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rusql-clone-{}-{}-{}",
            std::process::id(),
            label,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn clone_instance_copies_wal_and_replays() {
        let source = temp_dir("src");
        let dest_parent = temp_dir("dest-parent");
        let dest = dest_parent.join("clone");
        {
            let mut eng = PersistentEngine::open(&source).unwrap();
            eng.create_table(TableMeta {
                name: "t".into(),
                schema: "rusql".into(),
                columns: vec![ColumnDef::new("id", "INT")],
                auto_increment_next: None,
                ..Default::default()
            })
            .unwrap();
            eng.insert("t", vec!["1".into()]).unwrap();
        }
        assert!(source.join("rusql.wal").exists());
        clone_instance(&source, &dest).unwrap();
        assert!(dest.join("rusql.wal").exists());
        let eng = PersistentEngine::open(&dest).unwrap();
        assert_eq!(eng.scan("t").unwrap(), vec![vec!["1".to_string()]]);
        let _ = fs::remove_dir_all(&source);
        let _ = fs::remove_dir_all(&dest_parent);
    }

    #[test]
    fn clone_instance_rejects_non_empty_dest() {
        let source = temp_dir("src2");
        let dest = temp_dir("dest2");
        fs::write(dest.join("keep"), b"x").unwrap();
        match clone_instance(&source, &dest) {
            Err(CloneError::Mysql { code, .. }) => assert_eq!(code, ER_CLONE_SYS_CONFIG),
            other => panic!("expected errno 3869, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&source);
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn clone_instance_rejects_dest_inside_source() {
        let source = temp_dir("src3");
        let dest = source.join("nested");
        match clone_instance(&source, &dest) {
            Err(CloneError::Mysql { code, .. }) => assert_eq!(code, ER_CLONE_SYS_CONFIG),
            other => panic!("expected errno 3869, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&source);
    }
}

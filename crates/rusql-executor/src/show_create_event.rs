//! Documented `SHOW CREATE EVENT` stubs (M100).
//!
//! rusql has no event scheduler catalog yet. The statement is accepted
//! and returns MySQL `ER_EVENT_DOES_NOT_EXIST` until `CREATE EVENT` exists.

use crate::{ExecError, QueryResult};

pub(crate) const CREATE_EVENT_VIRTUAL_TABLE: &str = rusql_sql::CREATE_EVENT_VIRTUAL_TABLE;

/// MySQL `ER_EVENT_DOES_NOT_EXIST`.
const ER_EVENT_DOES_NOT_EXIST: u16 = 1539;

/// `SHOW CREATE EVENT [db.]name` — missing-event errno until a catalog exists.
pub(crate) fn show_create_event(
    _database: Option<&str>,
    name: &str,
) -> Result<QueryResult, ExecError> {
    Err(ExecError::Mysql {
        code: ER_EVENT_DOES_NOT_EXIST,
        message: rusql_i18n::messages::event_not_found(name),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_create_event_unknown_is_errno_1539() {
        match show_create_event(None, "e") {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, ER_EVENT_DOES_NOT_EXIST);
                assert!(message.contains("e"));
            }
            other => panic!("expected errno 1539, got {other:?}"),
        }
        match show_create_event(Some("rusql"), "no_such") {
            Err(ExecError::Mysql { code, message }) => {
                assert_eq!(code, ER_EVENT_DOES_NOT_EXIST);
                assert!(message.contains("no_such"));
            }
            other => panic!("expected errno 1539, got {other:?}"),
        }
    }
}

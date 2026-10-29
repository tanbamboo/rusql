//! ENUM / SET column validation and canonical storage (M148).

use rusql_core::{
    enum_set_members, store_spatial_cell, type_base, ColumnDef, EnumSetKind, GeometryFault,
    Session, TableMeta,
};
use rusql_storage::ColumnAssignment;

use crate::session_var::session_sql_mode;
use crate::ExecError;

const ER_DATA_TRUNCATED: u16 = 1265;

fn is_strict_mode(session: &Session) -> bool {
    let mode = session_sql_mode(session).to_ascii_uppercase();
    mode.contains("STRICT_TRANS_TABLES") || mode.contains("STRICT_ALL_TABLES")
}

fn truncated_error(column: &ColumnDef) -> ExecError {
    ExecError::Mysql {
        code: ER_DATA_TRUNCATED,
        message: rusql_i18n::messages::sql_data_truncated_for_column(&column.name),
    }
}

fn is_spatial_column(data_type: &str) -> bool {
    matches!(
        type_base(data_type).as_str(),
        "GEOMETRY"
            | "POINT"
            | "LINESTRING"
            | "POLYGON"
            | "MULTIPOINT"
            | "MULTILINESTRING"
            | "MULTIPOLYGON"
            | "GEOMCOLLECTION"
    )
}

fn coerce_spatial_cell(column: &ColumnDef, value: &str) -> Result<String, ExecError> {
    match store_spatial_cell(&column.data_type, value) {
        Ok(stored) => Ok(stored),
        Err(GeometryFault::SridMismatch {
            geometry,
            column_srid,
        }) => Err(ExecError::Mysql {
            code: 3643,
            message: rusql_i18n::messages::sql_gis_srid_mismatch(
                &column.name,
                geometry,
                column_srid,
            ),
        }),
        Err(GeometryFault::SrsNotFound(srid)) => Err(ExecError::Mysql {
            code: 3548,
            message: rusql_i18n::messages::sql_srs_not_found(srid),
        }),
        Err(_) => Err(ExecError::Mysql {
            code: 1416,
            message: rusql_i18n::messages::sql_gis_not_a_value(),
        }),
    }
}

fn find_member<'a>(members: &'a [String], value: &str) -> Option<&'a str> {
    members
        .iter()
        .find(|m| m.eq_ignore_ascii_case(value))
        .map(String::as_str)
}

/// Coerce a single cell for an ENUM/SET column; other types pass through unchanged.
pub fn coerce_enum_set_cell(
    session: &Session,
    column: &ColumnDef,
    value: &str,
) -> Result<String, ExecError> {
    if is_spatial_column(&column.data_type) {
        return coerce_spatial_cell(column, value);
    }
    let Some((kind, members)) = enum_set_members(&column.data_type) else {
        return Ok(value.to_string());
    };
    if value.is_empty() {
        return Ok(String::new());
    }
    let strict = is_strict_mode(session);
    match kind {
        EnumSetKind::Enum => coerce_enum_value(column, &members, value, strict),
        EnumSetKind::Set => coerce_set_value(column, &members, value, strict),
    }
}

fn coerce_enum_value(
    column: &ColumnDef,
    members: &[String],
    value: &str,
    strict: bool,
) -> Result<String, ExecError> {
    if let Some(m) = find_member(members, value) {
        return Ok(m.to_string());
    }
    if strict {
        return Err(truncated_error(column));
    }
    Ok(String::new())
}

fn coerce_set_value(
    column: &ColumnDef,
    members: &[String],
    value: &str,
    strict: bool,
) -> Result<String, ExecError> {
    let mut selected: Vec<String> = Vec::new();
    for part in value.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match find_member(members, part) {
            Some(m) => {
                if !selected.iter().any(|s| s.eq_ignore_ascii_case(m)) {
                    selected.push(m.to_string());
                }
            }
            None if strict => return Err(truncated_error(column)),
            None => {}
        }
    }
    let ordered: Vec<&str> = members
        .iter()
        .filter(|m| selected.iter().any(|s| s.eq_ignore_ascii_case(m.as_str())))
        .map(String::as_str)
        .collect();
    Ok(ordered.join(","))
}

pub fn coerce_column_assignments(
    session: &Session,
    meta: &TableMeta,
    assigns: &mut [ColumnAssignment],
) -> Result<(), ExecError> {
    for a in assigns {
        let col = meta
            .columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&a.column))
            .ok_or_else(|| {
                ExecError::Message(format!("Unknown column '{}' in assignment", a.column))
            })?;
        a.value = coerce_enum_set_cell(session, col, &a.value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusql_core::ColumnDef;

    use super::*;

    fn session_strict() -> Session {
        let mut s = Session::new(1, "root");
        s.session_vars
            .insert("sql_mode".into(), "STRICT_TRANS_TABLES".into());
        s
    }

    #[test]
    fn enum_type_valid_and_invalid() {
        let col = ColumnDef::new("e", "ENUM('a','b')");
        let session = session_strict();
        assert_eq!(coerce_enum_set_cell(&session, &col, "a").unwrap(), "a");
        assert_eq!(coerce_enum_set_cell(&session, &col, "A").unwrap(), "a");
        assert!(coerce_enum_set_cell(&session, &col, "c").is_err());
    }

    #[test]
    fn set_type_canonical_order() {
        let col = ColumnDef::new("s", "SET('a','b','c')");
        let session = session_strict();
        assert_eq!(coerce_enum_set_cell(&session, &col, "b,a").unwrap(), "a,b");
        assert_eq!(coerce_enum_set_cell(&session, &col, "c").unwrap(), "c");
        assert!(coerce_enum_set_cell(&session, &col, "d").is_err());
    }
}

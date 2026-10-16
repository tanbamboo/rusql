//! RANGE partition SELECT prune and EXPLAIN hints (M152).

use rusql_core::TableMeta;
use rusql_storage::StorageEngine;
use sqlparser::ast::Expr;

use crate::where_filter::{eq_predicate_from_filter, parse_where_filter};

pub fn prune_partitions_for_select(
    meta: Option<&TableMeta>,
    selection: Option<&Expr>,
) -> Option<Vec<String>> {
    let meta = meta?;
    let rp = meta.range_partition.as_ref()?;
    let filter = parse_where_filter(selection).ok().flatten()?;
    let (col, val) = eq_predicate_from_filter(&filter)?;
    rp.prune_for_eq(&col, &val)
}

pub fn scan_table_rows<E: StorageEngine>(
    engine: &E,
    table: &str,
    meta: Option<&TableMeta>,
    selection: Option<&Expr>,
) -> Result<Vec<rusql_storage::Row>, rusql_storage::StorageError> {
    let parts = prune_partitions_for_select(meta, selection);
    engine.scan_with_partition_prune(table, parts.as_deref())
}

pub fn explain_partition_extra(meta: Option<&TableMeta>, selection: Option<&Expr>) -> String {
    prune_partitions_for_select(meta, selection)
        .map(|parts| format!("partitions: {}", parts.join(",")))
        .unwrap_or_default()
}

pub fn pruned_row_count<E: StorageEngine>(
    engine: &E,
    table: &str,
    meta: Option<&TableMeta>,
    selection: Option<&Expr>,
) -> u64 {
    let parts = prune_partitions_for_select(meta, selection);
    engine
        .row_count_with_partition_prune(table, parts.as_deref())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use rusql_core::{
        ColumnDef, RangePartitionBound, RangePartitionDef, RangePartitionMeta, TableMeta,
    };
    use rusql_sql::parse;
    use rusql_storage::{HeapEngine, StorageEngine};
    use sqlparser::ast::{SetExpr, Statement};

    use super::*;

    fn range_meta() -> TableMeta {
        TableMeta {
            name: "t".into(),
            schema: "rusql".into(),
            columns: vec![ColumnDef::new("id", "INT")],
            range_partition: Some(RangePartitionMeta {
                key_column: "id".into(),
                partitions: vec![
                    RangePartitionDef {
                        name: "p0".into(),
                        less_than: RangePartitionBound::Value(10),
                    },
                    RangePartitionDef {
                        name: "p1".into(),
                        less_than: RangePartitionBound::MaxValue,
                    },
                ],
            }),
            ..Default::default()
        }
    }

    fn selection(sql: &str) -> Option<Expr> {
        let stmts = parse(sql).unwrap();
        let Statement::Query(q) = &stmts[0] else {
            panic!("query");
        };
        let SetExpr::Select(s) = q.body.as_ref() else {
            panic!("select");
        };
        s.selection.clone()
    }

    #[test]
    fn partition_range_explain_prunes_to_p0() {
        let meta = range_meta();
        let sel = selection("SELECT * FROM t WHERE id = 5");
        let extra = explain_partition_extra(Some(&meta), sel.as_ref());
        assert!(extra.contains("p0"));
        assert!(!extra.contains("p1"));
    }

    #[test]
    fn partition_range_select_eq_reads_one_partition() {
        let mut eng = HeapEngine::new();
        let meta = range_meta();
        eng.create_table(meta.clone()).unwrap();
        eng.insert("t", vec!["5".into()]).unwrap();
        eng.insert("t", vec!["20".into()]).unwrap();
        let sel = selection("SELECT * FROM t WHERE id = 5");
        let rows = scan_table_rows(&eng, "t", Some(&meta), sel.as_ref()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], "5");
    }
}

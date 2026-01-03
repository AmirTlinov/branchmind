#![forbid(unsafe_code)]

mod core;
mod evidence;
mod execution;
mod graph;
mod indexes;
mod ops_history;
mod pragmas;
mod reasoning;
mod tasks;

pub(super) fn full_schema_sql() -> String {
    let mut sql = String::new();
    sql.push_str(pragmas::SQL);
    sql.push_str(core::SQL);
    sql.push_str(tasks::SQL);
    sql.push_str(reasoning::SQL);
    sql.push_str(execution::SQL);
    sql.push_str(evidence::SQL);
    sql.push_str(ops_history::SQL);
    sql.push_str(graph::SQL);
    sql.push_str(indexes::SQL);
    sql
}

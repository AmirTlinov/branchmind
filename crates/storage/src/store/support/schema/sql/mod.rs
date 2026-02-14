#![forbid(unsafe_code)]

mod anchor_aliases;
mod anchor_bindings;
mod anchor_links;
mod anchors;
mod core;
mod evidence;
mod execution;
mod graph;
mod indexes;
mod job_artifacts;
mod job_bus;
mod jobs;
mod knowledge_keys;
mod ops_history;
mod pragmas;
mod reasoning;
mod runners;
mod tasks;

pub(super) fn full_schema_sql() -> String {
    let mut sql = String::new();
    sql.push_str(pragmas::SQL);
    sql.push_str(core::SQL);
    sql.push_str(tasks::SQL);
    sql.push_str(reasoning::SQL);
    sql.push_str(anchors::SQL);
    sql.push_str(anchor_aliases::SQL);
    sql.push_str(anchor_bindings::SQL);
    sql.push_str(anchor_links::SQL);
    sql.push_str(jobs::SQL);
    sql.push_str(job_artifacts::SQL);
    sql.push_str(job_bus::SQL);
    sql.push_str(runners::SQL);
    sql.push_str(execution::SQL);
    sql.push_str(evidence::SQL);
    sql.push_str(ops_history::SQL);
    sql.push_str(graph::SQL);
    sql.push_str(knowledge_keys::SQL);
    sql.push_str(indexes::SQL);
    sql
}

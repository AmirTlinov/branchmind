#![forbid(unsafe_code)]

mod add;
mod core;
mod graph_ops;
mod lint;
mod playbook;
mod query;
mod subgoals;
mod watch;

use serde_json::Value;

pub(crate) fn think_definitions() -> Vec<Value> {
    let mut out = Vec::new();

    out.extend(core::definitions());
    out.extend(add::definitions());
    out.extend(query::definitions());
    out.extend(graph_ops::definitions());
    out.extend(playbook::definitions());
    out.extend(subgoals::definitions());
    out.extend(watch::definitions());
    out.extend(lint::definitions());

    out
}

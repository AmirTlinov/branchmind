#![forbid(unsafe_code)]

use crate::*;
use serde_json::Value;

#[derive(Clone, Debug)]
pub(super) struct TimelineEvents {
    pub(super) limit: usize,
    pub(super) events: Vec<bm_storage::EventRow>,
}

pub(super) fn load_timeline_events(
    store: &mut bm_storage::SqliteStore,
    workspace: &WorkspaceId,
    target_id: &str,
    limit: usize,
) -> Result<TimelineEvents, Value> {
    let mut events = if limit == 0 {
        Vec::new()
    } else {
        match store.list_events_for_task(workspace, target_id, limit) {
            Ok(v) => v,
            Err(err) => return Err(ai_error("STORE_ERROR", &format_store_error(err))),
        }
    };
    events.reverse();
    sort_events_by_seq(&mut events);

    Ok(TimelineEvents { limit, events })
}

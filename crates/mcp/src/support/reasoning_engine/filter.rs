#![forbid(unsafe_code)]

use serde_json::Value;

pub(crate) fn filter_engine_to_cards(engine: &mut Value, cards: &[Value]) {
    let Some(obj) = engine.as_object_mut() else {
        return;
    };
    let _ = cards;

    // Important: do NOT prune engine signals/actions based on the visible card slice.
    //
    // Rationale:
    // - Meaning-mode hides drafts by default for low-noise UX, but the reasoning engine must still
    //   surface “hidden-but-important” discipline signals (BM4/BM9, publish hygiene, etc.).
    // - Strict gates and resume HUDs rely on these signals/actions even when the underlying cards
    //   are not included in the current output slice due to visibility or budgeting.
    //
    // Keeping refs intact is intentional: callers can disclose/include_drafts or open by id.
    let _ = obj;
}

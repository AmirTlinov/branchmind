#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GateMode {
    Strict,
    Deep,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct GateSpec {
    pub(super) mode: GateMode,
    pub(super) mode_label: &'static str,
    pub(super) playbook_name: &'static str,
    pub(super) playbook_hint: &'static str,
    pub(super) gate_tag: &'static str,
}

impl GateSpec {
    pub(super) fn for_task_reasoning_mode(raw: &str) -> Option<Self> {
        let raw = raw.trim().to_ascii_lowercase();
        match raw.as_str() {
            "strict" => Some(Self {
                mode: GateMode::Strict,
                mode_label: "strict",
                playbook_name: "strict",
                playbook_hint: "Load strict reasoning playbook (skepticism checklist).",
                gate_tag: "bm-strict",
            }),
            "deep" => Some(Self {
                mode: GateMode::Deep,
                mode_label: "deep",
                playbook_name: "deep",
                playbook_hint: "Load deep reasoning playbook (branch → falsify → merge).",
                gate_tag: "bm-deep",
            }),
            _ => None,
        }
    }
}

pub(super) fn status_is_closed(status: &str) -> bool {
    status.eq_ignore_ascii_case("closed")
        || status.eq_ignore_ascii_case("done")
        || status.eq_ignore_ascii_case("resolved")
}

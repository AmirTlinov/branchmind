#![forbid(unsafe_code)]

use super::items::ItemsPatch;
use super::node_row::ScalarPatch;

#[derive(Clone, Copy, Debug)]
pub(super) struct PatchPresence {
    pub(super) title: bool,
    pub(super) status: bool,
    pub(super) status_manual: bool,
    pub(super) priority: bool,
    pub(super) blocked: bool,
    pub(super) description: bool,
    pub(super) context: bool,
    pub(super) blockers: bool,
    pub(super) dependencies: bool,
    pub(super) next_steps: bool,
    pub(super) problems: bool,
    pub(super) risks: bool,
    pub(super) success_criteria: bool,
}

impl PatchPresence {
    pub(super) fn from_parts(scalar: &ScalarPatch, items: &ItemsPatch) -> Self {
        Self {
            title: scalar.title.is_some(),
            status: scalar.status.is_some(),
            status_manual: scalar.status_manual.is_some(),
            priority: scalar.priority.is_some(),
            blocked: scalar.blocked.is_some(),
            description: scalar.description.is_some(),
            context: scalar.context.is_some(),
            blockers: items.blockers.is_some(),
            dependencies: items.dependencies.is_some(),
            next_steps: items.next_steps.is_some(),
            problems: items.problems.is_some(),
            risks: items.risks.is_some(),
            success_criteria: items.success_criteria.is_some(),
        }
    }

    pub(super) fn any(&self) -> bool {
        self.title
            || self.status
            || self.status_manual
            || self.priority
            || self.blocked
            || self.description
            || self.context
            || self.blockers
            || self.dependencies
            || self.next_steps
            || self.problems
            || self.risks
            || self.success_criteria
    }

    pub(super) fn changed_fields(&self) -> Vec<&'static str> {
        let mut fields = Vec::new();
        if self.title {
            fields.push("title");
        }
        if self.status {
            fields.push("status");
        }
        if self.status_manual {
            fields.push("status_manual");
        }
        if self.priority {
            fields.push("priority");
        }
        if self.blocked {
            fields.push("blocked");
        }
        if self.description {
            fields.push("description");
        }
        if self.context {
            fields.push("context");
        }
        if self.blockers {
            fields.push("blockers");
        }
        if self.dependencies {
            fields.push("dependencies");
        }
        if self.next_steps {
            fields.push("next_steps");
        }
        if self.problems {
            fields.push("problems");
        }
        if self.risks {
            fields.push("risks");
        }
        if self.success_criteria {
            fields.push("success_criteria");
        }
        fields
    }
}

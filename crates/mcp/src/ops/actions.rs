#![forbid(unsafe_code)]

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionPriority {
    High,
    Medium,
    Low,
}

impl ActionPriority {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ActionPriority::High => "high",
            ActionPriority::Medium => "medium",
            ActionPriority::Low => "low",
        }
    }

    pub(crate) fn rank(self) -> u8 {
        match self {
            ActionPriority::High => 0,
            ActionPriority::Medium => 1,
            ActionPriority::Low => 2,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Action {
    pub(crate) action_id: String,
    pub(crate) priority: ActionPriority,
    pub(crate) tool: String,
    pub(crate) args: Value,
    pub(crate) why: String,
    pub(crate) risk: String,
}

impl Action {
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "action_id": self.action_id,
            "priority": self.priority.as_str(),
            "tool": self.tool,
            "args": self.args,
            "why": self.why,
            "risk": self.risk,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_rank_is_stable_and_total_ordered() {
        assert_eq!(ActionPriority::High.as_str(), "high");
        assert_eq!(ActionPriority::Medium.as_str(), "medium");
        assert_eq!(ActionPriority::Low.as_str(), "low");

        assert!(ActionPriority::High.rank() < ActionPriority::Medium.rank());
        assert!(ActionPriority::Medium.rank() < ActionPriority::Low.rank());
    }
}

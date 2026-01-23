#![forbid(unsafe_code)]

mod support;

#[path = "branchmind_core/bootstrap.rs"]
mod bootstrap;
#[path = "branchmind_core/branching.rs"]
mod branching;
#[path = "branchmind_core/export.rs"]
mod export;
#[path = "branchmind_core/memory.rs"]
mod memory;
#[path = "branchmind_core/merge.rs"]
mod merge;
#[path = "branchmind_core/skill.rs"]
mod skill;
#[path = "branchmind_core/snapshot.rs"]
mod snapshot;
#[path = "branchmind_core/templates.rs"]
mod templates;
#[path = "branchmind_core/vcs.rs"]
mod vcs;

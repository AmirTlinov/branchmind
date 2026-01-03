#![forbid(unsafe_code)]

mod support;

#[path = "tasks_contract/create.rs"]
mod create;
#[path = "tasks_contract/flows/mod.rs"]
mod flows;
#[path = "tasks_contract/steps/mod.rs"]
mod steps;
#[path = "tasks_contract/views/mod.rs"]
mod views;

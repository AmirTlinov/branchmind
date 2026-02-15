#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod dto;
mod support;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::projects_scan,
            commands::workspaces_list,
            commands::focus_get,
            commands::tasks_list,
            commands::tasks_get,
            commands::plans_get,
            commands::reasoning_ref_get,
            commands::steps_list,
            commands::steps_detail,
            commands::task_steps_summary,
            commands::docs_entries_since,
            commands::docs_show_tail,
            commands::branches_list,
            commands::graph_query,
            commands::graph_diff,
            commands::tasks_search,
            commands::knowledge_search,
            commands::knowledge_card_get,
            commands::anchors_list,
            commands::architecture_lens_get,
            commands::architecture_provenance_get,
            commands::architecture_hotspots_get,
        ])
        .run(tauri::generate_context!())
        .expect("tauri run");
}

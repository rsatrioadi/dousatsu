mod color;
mod graph;

use std::sync::Mutex;
use tauri::Manager;

pub struct AppState {
    pub elements: Mutex<Option<graph::Elements>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            app.manage(AppState {
                elements: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            graph::parse_graph,
            graph::build_hierarchy,
            graph::get_neighbors,
            graph::get_node,
            graph::get_focused_view,
            color::compute_coloring_gradient,
            color::compute_coloring_by_dimension,
            color::list_dimensions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

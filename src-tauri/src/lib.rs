mod controllers;
mod models;

// See the Tauri docs to learn more about commands like this one
// https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(controllers::simulation_session_controller::SessionState::default())
        .invoke_handler(tauri::generate_handler![
            greet,
            controllers::editor_controller::compile,
            controllers::editor_controller::default_open_dir,
            controllers::simulation_session_controller::start_simulation,
            controllers::simulation_session_controller::pause_simulation,
            controllers::simulation_session_controller::resume_simulation,
            controllers::simulation_session_controller::step_action,
            controllers::simulation_session_controller::step_instruction,
            controllers::simulation_session_controller::stop_simulation,
            controllers::simulation_session_controller::set_speed,
            controllers::simulation_session_controller::set_breakpoints,
            controllers::memory_contents_controller::get_registers,
            controllers::memory_contents_controller::get_cache_level_info,
            controllers::memory_contents_controller::get_cache_sets,
            controllers::memory_contents_controller::get_cache_lines,
            controllers::memory_contents_controller::get_ram_blocks,
            controllers::memory_contents_controller::get_tlb_info,
            controllers::memory_contents_controller::get_tlb_sets,
            controllers::memory_contents_controller::get_frames,
            controllers::memory_contents_controller::get_frame_lines,
            controllers::memory_contents_controller::get_page_table_entries,
            controllers::memory_contents_controller::get_page_table_entry_count,
            controllers::memory_contents_controller::get_page_table_levels,
            controllers::memory_contents_controller::get_page_table_level_bits,
            controllers::memory_contents_controller::get_disk_pages,
            controllers::stats_controller::get_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

mod controllers;
mod models;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
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
        .manage(controllers::simulation_controller::SessionState::default())
        .invoke_handler(tauri::generate_handler![
            greet,
            controllers::editor_controller::compile,
            controllers::simulation_controller::start_simulation,
            controllers::simulation_controller::pause_simulation,
            controllers::simulation_controller::resume_simulation,
            controllers::simulation_controller::step_action,
            controllers::simulation_controller::step_instruction,
            controllers::simulation_controller::stop_simulation,
            controllers::simulation_controller::set_speed,
            controllers::simulation_controller::get_registers,
            controllers::simulation_controller::get_cache_level,
            controllers::simulation_controller::get_ram_blocks,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

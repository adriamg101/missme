// Stops an extra console window from popping up on Windows in the release
// build. Do not remove this.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    missme_lib::run()
}

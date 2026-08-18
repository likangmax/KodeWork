#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = kodework_tauri::run() {
        eprintln!("Kodework failed to start: {error}");
    }
}

// Точка входа Windows (в tauri 2 main.rs может быть пустым при lib-схеме).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    convert_desktop_lib::run()
}

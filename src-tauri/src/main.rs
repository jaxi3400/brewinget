#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|a| a == "--auto-update") {
        brewinget_lib::run_headless();
    } else {
        brewinget_lib::run();
    }
}

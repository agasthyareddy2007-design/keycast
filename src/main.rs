#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod input;
mod network;
mod protocol;
mod state;
mod ui;
fn main() {
    ui::run();
}

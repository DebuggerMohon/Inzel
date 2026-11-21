#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Inzel - push-to-talk voice typing and translator for Windows.

mod audio;
mod config;
mod engine;
mod events;
mod gfx;
mod hotkeys;
mod i18n;
mod inject;
mod languages;
mod net;
mod overlay;
mod stt;
mod system;
mod theme;
mod translate;
mod tray;
mod ui;
mod util;

pub const APP_NAME: &str = "Inzel";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHOR: &str = "Debugger Mohon";
pub const GITHUB_URL: &str = "https://github.com/DebuggerMohon/Inzel";

fn main() {
    if !system::single_instance() {
        system::activate_existing();
        return;
    }
    ui::run(std::env::args().any(|a| a == "--minimized"));
}

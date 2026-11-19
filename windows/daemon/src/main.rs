#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use nmm_core::config::{explicit_config_path, resolve_config_path};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Runs the one-time migration from the old location, if needed.
    let _ = resolve_config_path(args.iter().cloned());
    // `--config <path>` / NMM_CONFIG force a file; otherwise the daemon follows the path saved in the GUI.
    match nmm_platform_windows::run(explicit_config_path(args)) {
        Ok(_) => {}
        Err(e) => nmm_platform_windows::error_box("not-magick-mouse", &format!("{e:#}")),
    }
}

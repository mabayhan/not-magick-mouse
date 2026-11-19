//! Linux backend: evdev grab + uinput re-injection (works on X11 and Wayland),
//! X11 focus detection (no focus detection on pure Wayland: only the global
//! and per-mouse layers apply there), a StatusNotifierItem tray icon, and the
//! helpers the GUI uses.
#![cfg(target_os = "linux")]

mod control;
mod daemon;
mod devices;
mod focus;
mod ipc;
mod keymap;
mod mouse;
mod output;
mod tray;

pub use daemon::run;
pub use devices::{MouseInfo, list_mice, list_running_apps};
pub use ipc::{notify_reload, set_remap_suspended, single_instance};
pub use keymap::key_code;

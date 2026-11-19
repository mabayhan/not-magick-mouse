//! Linux backend: evdev grab + uinput re-injection (works on X11 and Wayland),
//! X11 focus detection (no focus detection on pure Wayland: only the global
//! and per-mouse layers apply there).
#![cfg(target_os = "linux")]

mod daemon;
mod focus;
mod keymap;
mod mouse;
mod output;

pub use daemon::run;
pub use keymap::key_code;

//! Windows backend: hook, injector, foreground watcher, tray, OSD, Razer DPI.

mod app;
mod dpi;
mod inject;
mod keymap;
mod osd;
mod util;

pub use app::run;
pub use keymap::key_vk;
pub use util::{error_box, list_running_apps, notify_reload, set_remap_suspended, single_instance};

//! Switches the tray flips and the device threads read.

use std::sync::atomic::AtomicBool;

pub struct Control {
    /// Remapping on/off (off = every button passes through).
    pub enabled: AtomicBool,
    /// Set by the tray's "Reload config"; the config watcher clears it.
    pub reload: AtomicBool,
}

impl Control {
    pub fn new() -> Control {
        Control { enabled: AtomicBool::new(true), reload: AtomicBool::new(false) }
    }
}

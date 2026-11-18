//! Seams for OS backends. The Windows crate implements these today; a Linux
//! backend (evdev/uinput, X11/Wayland focus) can implement them later.

use crate::action::Action;
use crate::button::Button;

/// Synthesises input and runs actions.
pub trait Injector {
    fn perform(&self, action: &Action);
    fn click(&self, button: Button, down: bool);
}

/// Reports the focused application's executable name.
pub trait FocusWatcher {
    fn current_exe(&self) -> Option<String>;
}

/// Reads the mouse's current DPI.
pub trait DpiSource {
    fn read_dpi(&mut self) -> Option<u32>;
}

/// Shows a transient on-screen message.
pub trait Osd {
    fn show(&self, text: &str, millis: u32);
}

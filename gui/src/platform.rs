//! The few things the settings window asks the operating system for.

#[cfg(windows)]
pub use nmm_platform_windows::{MouseInfo, list_mice, list_running_apps, notify_reload, set_remap_suspended};

#[cfg(target_os = "linux")]
pub use nmm_platform_linux::{MouseInfo, list_mice, list_running_apps, notify_reload, set_remap_suspended};

/// Reading and changing a mouse's DPI is implemented on Windows only.
pub const HAS_DPI: bool = cfg!(windows);

#[cfg(windows)]
pub const EXAMPLE_CONFIG_PATH: &str = "D:\\tools\\nmm\\config.toml";
#[cfg(not(windows))]
pub const EXAMPLE_CONFIG_PATH: &str = "/home/you/nmm/config.toml";

/// Hint for the "type an application's file name" box.
#[cfg(windows)]
pub const APP_HINT: &str = "name.exe";
#[cfg(not(windows))]
pub const APP_HINT: &str = "firefox";

/// Hint for the "run a program" action.
#[cfg(windows)]
pub const RUN_HINT: &str = "C:\\path\\app.exe arguments";
#[cfg(not(windows))]
pub const RUN_HINT: &str = "/usr/bin/app arguments";

/// False when another settings window is already open.
pub fn single_instance() -> bool {
    #[cfg(windows)]
    return nmm_platform_windows::single_instance("Local\\not-magick-mouse-gui");
    #[cfg(target_os = "linux")]
    return nmm_platform_linux::single_instance("nmm-gui");
}

/// Window styling that has to happen once the window exists.
pub fn style_window(cc: &eframe::CreationContext<'_>) {
    #[cfg(windows)]
    dark_title_bar(cc);
    #[cfg(not(windows))]
    let _ = cc;
}

/// Give the window a dark title bar from the start, with the same colour whether or not
/// the window has focus (Windows 11 otherwise shows white, then black, then dark grey).
#[cfg(windows)]
fn dark_title_bar(cc: &eframe::CreationContext<'_>) {
    use crate::theme::{BORDER, SIDEBAR, TEXT};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::*;
    let Ok(h) = cc.window_handle() else { return };
    let RawWindowHandle::Win32(w) = h.as_raw() else { return };
    let hwnd = HWND(w.hwnd.get() as *mut _);
    let colorref = |c: eframe::egui::Color32| u32::from(c.r()) | (u32::from(c.g()) << 8) | (u32::from(c.b()) << 16);
    let set = |attr: DWMWINDOWATTRIBUTE, v: u32| unsafe {
        let _ = DwmSetWindowAttribute(hwnd, attr, &v as *const u32 as *const _, 4);
    };
    set(DWMWA_USE_IMMERSIVE_DARK_MODE, 1);
    set(DWMWA_CAPTION_COLOR, colorref(SIDEBAR)); // Windows 11 only; ignored elsewhere
    set(DWMWA_BORDER_COLOR, colorref(BORDER));
    set(DWMWA_TEXT_COLOR, colorref(TEXT));
}

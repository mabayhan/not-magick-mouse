//! Knowing which physical mouse a click came from.
//!
//! The low-level mouse hook (what remaps buttons) does not say which device
//! produced an event. Raw Input does, so the daemon also registers for raw
//! mouse input and keeps a short list of recent button transitions tagged
//! with the device. The hook asks for the matching entry. When no entry is
//! found the click counts as coming from an unknown mouse, and only the
//! global profile applies: remapping never depends on this lookup working.

use nmm_core::Button;
use nmm_core::config::device_key;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM};
use windows::Win32::UI::Input::*;

/// A mouse seen by Windows.
#[derive(Debug, Clone, PartialEq)]
pub struct MouseInfo {
    /// `vid_xxxx&pid_xxxx`, the key used by `[[device]]` profiles in the config.
    pub key: String,
    /// Friendly name.
    pub name: String,
    /// Can the daemon read and set its DPI (Razer).
    pub dpi_capable: bool,
}

fn vendor_name(key: &str) -> Option<&'static str> {
    let v = key.strip_prefix("vid_").or_else(|| key.strip_prefix("ven_"))?.get(..4)?;
    Some(match v {
        "1532" => "Razer",
        "046d" => "Logitech",
        "045e" => "Microsoft",
        "06cb" => "Touchpad (Synaptics)",
        "04f3" => "Touchpad (Elan)",
        "1038" => "SteelSeries",
        "1b1c" => "Corsair",
        _ => return None,
    })
}

fn is_virtual(path: &str) -> bool {
    let p = path.to_ascii_uppercase();
    p.contains("ROOT#RDP") || p.contains("RDP_MOU") || p.contains("VIRTUAL")
}

fn device_path(h: HANDLE) -> Option<String> {
    unsafe {
        let mut len = 0u32;
        GetRawInputDeviceInfoW(Some(h), RIDI_DEVICENAME, None, &mut len);
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetRawInputDeviceInfoW(Some(h), RIDI_DEVICENAME, Some(buf.as_mut_ptr() as *mut _), &mut len);
        if n == 0 || n == u32::MAX {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..(n as usize).min(buf.len())]).trim_end_matches('\0').to_string())
    }
}

/// Product names reported by the HID layer, by key, and the keys of Logitech
/// mice that expose the HID++ vendor collection (so their DPI can be set).
fn product_names() -> (HashMap<String, String>, std::collections::HashSet<String>) {
    let mut out = HashMap::new();
    let mut hidpp = std::collections::HashSet::new();
    if let Ok(api) = hidapi::HidApi::new() {
        for d in api.device_list() {
            if d.vendor_id() == crate::logitech::LOGI_VID && d.usage_page() == crate::logitech::VENDOR_PAGE {
                if let Some(k) = device_key(&d.path().to_string_lossy()) {
                    hidpp.insert(k);
                }
            }
            let Some(p) = d.product_string().map(str::trim).filter(|p| !p.is_empty()) else { continue };
            if let Some(k) = device_key(&d.path().to_string_lossy()) {
                // The mouse collection's name beats a keyboard collection's on the same receiver.
                let better = d.usage_page() == 1 && d.usage() == 2;
                if better || !out.contains_key(&k) {
                    out.insert(k, p.to_string());
                }
            }
        }
    }
    (out, hidpp)
}

/// Mice currently known to Windows, one entry per model (identical mice share a key).
pub fn list_mice() -> Vec<MouseInfo> {
    let mut handles: Vec<RAWINPUTDEVICELIST> = Vec::new();
    unsafe {
        let mut n = 0u32;
        let sz = size_of::<RAWINPUTDEVICELIST>() as u32;
        if GetRawInputDeviceList(None, &mut n, sz) == u32::MAX || n == 0 {
            return Vec::new();
        }
        handles.resize(n as usize, RAWINPUTDEVICELIST::default());
        let got = GetRawInputDeviceList(Some(handles.as_mut_ptr()), &mut n, sz);
        if got == u32::MAX {
            return Vec::new();
        }
        handles.truncate(got as usize);
    }
    let (names, hidpp) = product_names();
    let mut out: Vec<MouseInfo> = Vec::new();
    for h in handles.iter().filter(|h| h.dwType == RIM_TYPEMOUSE) {
        let Some(path) = device_path(h.hDevice) else { continue };
        if is_virtual(&path) {
            continue;
        }
        let Some(key) = device_key(&path) else { continue };
        if out.iter().any(|m| m.key == key) {
            continue;
        }
        let name = names
            .get(&key)
            .cloned()
            .or_else(|| vendor_name(&key).map(|v| format!("{v} mouse")))
            .unwrap_or_else(|| format!("Mouse ({key})"));
        let dpi_capable = key.starts_with("vid_1532") || hidpp.contains(&key);
        out.push(MouseInfo { key, name, dpi_capable });
    }
    out
}

// ---------- click source tracking ----------

const RI_MOUSE_LEFT_DOWN: u16 = 0x0001;
const RI_MOUSE_RIGHT_DOWN: u16 = 0x0004;
const RI_MOUSE_MIDDLE_DOWN: u16 = 0x0010;
const RI_MOUSE_BUTTON_4_DOWN: u16 = 0x0040;
const RI_MOUSE_BUTTON_5_DOWN: u16 = 0x0100;

/// Raw button flags -> (button, pressed).
fn decode(flags: u16) -> Vec<(Button, bool)> {
    let mut v = Vec::new();
    for (b, down_bit) in [
        (Button::Left, RI_MOUSE_LEFT_DOWN),
        (Button::Right, RI_MOUSE_RIGHT_DOWN),
        (Button::Middle, RI_MOUSE_MIDDLE_DOWN),
        (Button::X1, RI_MOUSE_BUTTON_4_DOWN),
        (Button::X2, RI_MOUSE_BUTTON_5_DOWN),
    ] {
        if flags & down_bit != 0 {
            v.push((b, true));
        }
        if flags & (down_bit << 1) != 0 {
            v.push((b, false));
        }
    }
    v
}

struct Rec {
    button: Button,
    down: bool,
    key: Option<String>,
    at: Instant,
}

const MAX_AGE: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct Tracker {
    names: HashMap<isize, Option<String>>,
    recent: VecDeque<Rec>,
}

impl Tracker {
    fn push(&mut self, device: HANDLE, flags: u16) {
        if device.0.is_null() {
            return; // injected input (our own, or other software's) has no device
        }
        let hdev = device.0 as isize;
        let key = self.names.entry(hdev).or_insert_with(|| device_path(device).and_then(|p| device_key(&p))).clone();
        for (button, down) in decode(flags) {
            self.recent.push_back(Rec { button, down, key: key.clone(), at: Instant::now() });
        }
        while self.recent.len() > 32 {
            self.recent.pop_front();
        }
    }

    /// The mouse behind the newest unclaimed `button` transition, if one was seen just now.
    pub fn take(&mut self, button: Button, down: bool) -> Option<String> {
        self.recent.retain(|r| r.at.elapsed() < MAX_AGE);
        let i = self.recent.iter().rposition(|r| r.button == button && r.down == down)?;
        self.recent.remove(i)?.key
    }

    /// Pull any raw mouse input already queued for this thread. Called from
    /// the hook, so the matching record is available when the hook runs.
    pub fn drain(&mut self) {
        // 8-byte aligned scratch space for a few RAWINPUT records.
        let mut buf = [0u64; 128];
        loop {
            let mut size = size_of_val(&buf) as u32;
            let n = unsafe { GetRawInputBuffer(Some(buf.as_mut_ptr() as *mut RAWINPUT), &mut size, size_of::<RAWINPUTHEADER>() as u32) };
            if n == 0 || n == u32::MAX {
                break;
            }
            let mut p = buf.as_ptr() as *const u8;
            for _ in 0..n {
                let ri = unsafe { &*(p as *const RAWINPUT) };
                if ri.header.dwType == RIM_TYPEMOUSE.0 {
                    let m = unsafe { ri.data.mouse };
                    let flags = unsafe { m.Anonymous.Anonymous.usButtonFlags };
                    self.push(ri.header.hDevice, flags);
                }
                // NEXTRAWINPUTBLOCK: advance by dwSize, rounded up to 8 bytes.
                let step = (ri.header.dwSize as usize + 7) & !7;
                p = unsafe { p.add(step) };
            }
        }
    }

    /// Handle one `WM_INPUT` message (the same data, delivered the normal way).
    pub fn on_wm_input(&mut self, lparam: LPARAM) {
        let mut buf = [0u64; 16];
        let mut size = size_of_val(&buf) as u32;
        let n = unsafe { GetRawInputData(HRAWINPUT(lparam.0 as *mut _), RID_INPUT, Some(buf.as_mut_ptr() as *mut _), &mut size, size_of::<RAWINPUTHEADER>() as u32) };
        if n == u32::MAX || n == 0 {
            return;
        }
        let ri = unsafe { &*(buf.as_ptr() as *const RAWINPUT) };
        if ri.header.dwType == RIM_TYPEMOUSE.0 {
            let flags = unsafe { ri.data.mouse.Anonymous.Anonymous.usButtonFlags };
            self.push(ri.header.hDevice, flags);
        }
    }
}

/// Ask Windows to deliver raw mouse input to `hwnd` even when it is in the background.
pub fn register_raw_mouse(hwnd: HWND) {
    let rid = RAWINPUTDEVICE { usUsagePage: 1, usUsage: 2, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
    unsafe {
        let _ = RegisterRawInputDevices(&[rid], size_of::<RAWINPUTDEVICE>() as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_decode() {
        assert_eq!(decode(0x0040), vec![(Button::X1, true)]);
        assert_eq!(decode(0x0080), vec![(Button::X1, false)]);
        assert_eq!(decode(0x0001 | 0x0008), vec![(Button::Left, true), (Button::Right, false)]);
        assert!(decode(0x0400).is_empty()); // wheel
    }

    #[test]
    fn take_matches_and_consumes() {
        let mut t = Tracker::default();
        t.recent.push_back(Rec { button: Button::X1, down: true, key: Some("vid_1532&pid_005c".into()), at: Instant::now() });
        assert_eq!(t.take(Button::X2, true), None);
        assert_eq!(t.take(Button::X1, false), None);
        assert_eq!(t.take(Button::X1, true).as_deref(), Some("vid_1532&pid_005c"));
        assert_eq!(t.take(Button::X1, true), None); // consumed
    }

    #[test]
    fn stale_records_are_ignored() {
        let mut t = Tracker::default();
        t.recent.push_back(Rec { button: Button::Left, down: true, key: Some("k".into()), at: Instant::now() - Duration::from_secs(1) });
        assert_eq!(t.take(Button::Left, true), None);
    }

    #[test]
    fn vendors() {
        assert_eq!(vendor_name("vid_1532&pid_005c"), Some("Razer"));
        assert_eq!(vendor_name("ven_06cb"), Some("Touchpad (Synaptics)"));
        assert_eq!(vendor_name("vid_ffff&pid_0001"), None);
    }
}

#[cfg(test)]
mod live {
    #[test]
    #[ignore = "prints the mice on this machine"]
    fn print_mice() {
        for m in super::list_mice() {
            println!("{m:?}");
        }
    }
}

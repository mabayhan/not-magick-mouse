//! What the GUI needs to know about the system: the mice and the running applications.
//! Both work without special permissions (no /dev/input access, no grabbing).

use crate::focus::exe_name;
use crate::output::VIRTUAL_PREFIX;
use std::collections::HashSet;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

/// A mouse seen by the kernel.
#[derive(Debug, Clone, PartialEq)]
pub struct MouseInfo {
    /// `vid_xxxx&pid_xxxx`, the key used by `[[device]]` profiles in the config.
    pub key: String,
    pub name: String,
    /// Reading and setting DPI is not supported on Linux yet.
    pub dpi_capable: bool,
}

/// Mice from the text of `/proc/bus/input/devices`: a device is a mouse when it has a
/// `mouse` handler and reports relative X/Y motion. Identical vendor/product pairs are listed once.
pub fn parse_mice(text: &str) -> Vec<MouseInfo> {
    let mut out: Vec<MouseInfo> = Vec::new();
    let mut seen = HashSet::new();
    for block in text.split("\n\n") {
        let (mut vendor, mut product, mut name, mut mouse, mut rel) = (None, None, None, false, false);
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("I: ") {
                for part in rest.split_whitespace() {
                    if let Some(v) = part.strip_prefix("Vendor=") {
                        vendor = u16::from_str_radix(v, 16).ok();
                    } else if let Some(p) = part.strip_prefix("Product=") {
                        product = u16::from_str_radix(p, 16).ok();
                    }
                }
            } else if let Some(n) = line.strip_prefix("N: Name=") {
                name = Some(n.trim().trim_matches('"').to_string());
            } else if let Some(h) = line.strip_prefix("H: Handlers=") {
                mouse = h.split_whitespace().any(|w| w.strip_prefix("mouse").is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit())));
            } else if let Some(r) = line.strip_prefix("B: REL=") {
                // Hex words, most significant first: the last one holds REL_X (bit 0) and REL_Y (bit 1).
                let last = r.split_whitespace().last().and_then(|w| u64::from_str_radix(w, 16).ok()).unwrap_or(0);
                rel = last & 0b11 == 0b11;
            }
        }
        let (Some(vendor), Some(product), Some(name)) = (vendor, product, name) else { continue };
        if !mouse || !rel || name.starts_with(VIRTUAL_PREFIX) {
            continue;
        }
        let key = format!("vid_{vendor:04x}&pid_{product:04x}");
        if seen.insert(key.clone()) {
            out.push(MouseInfo { key, name, dpi_capable: false });
        }
    }
    out
}

pub fn list_mice() -> Vec<MouseInfo> {
    std::fs::read_to_string("/proc/bus/input/devices").map(|t| parse_mice(&t)).unwrap_or_default()
}

/// Distinct executable names of the windows open on an X11 (or XWayland) display,
/// for the GUI's app picker. Empty on pure Wayland sessions.
pub fn list_running_apps() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Ok((conn, screen)) = x11rb::connect(None) else { return out };
    let root = conn.setup().roots[screen].root;
    let atom = |name: &[u8]| conn.intern_atom(false, name).ok().and_then(|c| c.reply().ok()).map(|r| r.atom);
    let (Some(list), Some(pid_atom)) = (atom(b"_NET_CLIENT_LIST"), atom(b"_NET_WM_PID")) else { return out };
    let Some(windows) = conn.get_property(false, root, list, AtomEnum::WINDOW, 0, 4096).ok().and_then(|c| c.reply().ok()) else { return out };
    for win in windows.value32().into_iter().flatten() {
        let pid = conn.get_property(false, win, pid_atom, AtomEnum::CARDINAL, 0, 1).ok().and_then(|c| c.reply().ok()).and_then(|r| r.value32().and_then(|mut v| v.next()));
        if let Some(name) = pid.and_then(exe_name)
            && !out.iter().any(|x| x.eq_ignore_ascii_case(&name))
        {
            out.push(name);
        }
    }
    out.sort_by_key(|s| s.to_ascii_lowercase());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
I: Bus=0011 Vendor=0001 Product=0001 Version=ab41
N: Name=\"AT Translated Set 2 keyboard\"
H: Handlers=sysrq kbd event0 leds
B: EV=120013
B: KEY=402000000 3803078f800d001 feffffdfffefffff fffffffffffffffe

I: Bus=0003 Vendor=1532 Product=005c Version=0111
N: Name=\"Razer Razer DeathAdder Elite\"
H: Handlers=mouse0 event5
B: EV=17
B: KEY=ffff0000 0 0 0 0
B: REL=943
B: MSC=10

I: Bus=0003 Vendor=1532 Product=005c Version=0111
N: Name=\"Razer Razer DeathAdder Elite Keyboard\"
H: Handlers=sysrq kbd event6 leds
B: EV=12001f

I: Bus=0003 Vendor=1532 Product=005c Version=0111
N: Name=\"nmm virtual Razer Razer DeathAdder Elite\"
H: Handlers=mouse1 event12
B: EV=7
B: REL=943

I: Bus=0011 Vendor=0002 Product=0007 Version=01b1
N: Name=\"SynPS/2 Synaptics TouchPad\"
H: Handlers=mouse2 event9
B: EV=b
B: KEY=e520 10000 0 0 0 0
B: ABS=660800011000003
";

    #[test]
    fn finds_only_real_mice() {
        let mice = parse_mice(SAMPLE);
        assert_eq!(mice.len(), 1);
        assert_eq!(mice[0].key, "vid_1532&pid_005c");
        assert_eq!(mice[0].name, "Razer Razer DeathAdder Elite");
    }
}

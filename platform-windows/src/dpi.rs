//! Razer DPI reader/writer for any number of connected Razer mice. Verified on
//! a DeathAdder Elite (PID 005C): the mouse collection (HID interface 0)
//! answers "get DPI" (class 0x04, command 0x85) and accepts "set DPI" (0x05),
//! transaction id 0x3F. The mouse emits no event on a DPI change and refuses
//! to store a custom stage list (0x06), so we poll, and when the DPI button
//! moves the firmware to a new value we replace it with the next value from
//! the user's own list (per mouse, falling back to the global list).

use hidapi::{DeviceInfo, HidApi, HidDevice};
use crate::logitech::{self, Hidpp};
use nmm_core::config::device_key;
use nmm_core::{Config, DpiCmd};
use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub const RAZER_VID: u16 = 0x1532;
const TXN: u8 = 0x3F;
const RESCAN: Duration = Duration::from_secs(5);

/// Something to show on the OSD: a DPI change on one mouse, or a problem.
pub struct DpiEvent {
    pub name: String,
    pub dpi: u32,
    /// More than one DPI-capable mouse is connected, so the OSD should say which.
    pub multi: bool,
    /// A message instead of a DPI value (e.g. the mouse could not be reached).
    pub note: Option<String>,
}

/// Used when a mouse has no DPI list of its own and there is no global one.
const DEFAULT_STAGES: [u32; 4] = [400, 800, 1600, 3200];

#[derive(Default)]
pub struct Shared {
    poll_ms: AtomicU32,
    cfg: Mutex<Config>,
    events: Mutex<Vec<DpiEvent>>,
    /// DPI values we set ourselves (by mouse key), so the poller does not mistake them for the DPI button.
    expected: Mutex<HashMap<String, u32>>,
    commands: Mutex<Option<mpsc::Sender<(Option<String>, DpiCmd)>>>,
}

impl Shared {
    /// `poll_ms == 0` pauses all DPI handling.
    pub fn configure(&self, cfg: &Config) {
        self.poll_ms.store(cfg.settings.dpi_poll_ms, Ordering::Relaxed);
        if let Ok(mut c) = self.cfg.lock() {
            *c = cfg.clone();
        }
    }

    /// Events queued since the last call.
    pub fn take_events(&self) -> Vec<DpiEvent> {
        self.events.lock().map(|mut e| std::mem::take(&mut *e)).unwrap_or_default()
    }

    /// Ask the DPI worker to change the DPI of the mouse behind `key` (`None` = not identified).
    pub fn request(&self, key: Option<String>, cmd: DpiCmd) {
        if let Ok(g) = self.commands.lock() {
            if let Some(tx) = g.as_ref() {
                let _ = tx.send((key, cmd));
            }
        }
    }

    fn stages_for(&self, key: &str) -> Vec<u32> {
        self.cfg.lock().map(|c| c.dpi_stages_for(key).to_vec()).unwrap_or_default()
    }
}

fn packet(cmd: u8, args: &[u8]) -> [u8; 91] {
    // [0] = HID report id, then the 90-byte Razer packet.
    let mut b = [0u8; 91];
    b[2] = TXN;
    b[6] = 0x07; // data size
    b[7] = 0x04; // command class: DPI
    b[8] = cmd;
    b[9..9 + args.len()].copy_from_slice(args);
    let mut crc = 0u8;
    for x in &b[3..89] {
        crc ^= x;
    }
    b[89] = crc;
    b
}

fn query(dev: &HidDevice) -> Option<u32> {
    dev.send_feature_report(&packet(0x85, &[0x01])).ok()?;
    std::thread::sleep(Duration::from_millis(20));
    let mut resp = [0u8; 91];
    let n = dev.get_feature_report(&mut resp).ok()?;
    if n < 14 || resp[1] != 0x02 {
        return None;
    }
    let x = u16::from_be_bytes([resp[10], resp[11]]) as u32;
    (100..=40000).contains(&x).then_some(x)
}

fn set_dpi(dev: &HidDevice, dpi: u32) -> bool {
    let [h, l] = (dpi.min(u16::MAX as u32) as u16).to_be_bytes();
    if dev.send_feature_report(&packet(0x05, &[0x01, h, l, h, l, 0, 0])).is_err() {
        return false;
    }
    std::thread::sleep(Duration::from_millis(20));
    let mut resp = [0u8; 91];
    dev.get_feature_report(&mut resp).is_ok() && resp[1] == 0x02
}

/// The value after `current` in `stages` (wrapping). If `current` is not in
/// the list, the smallest stage above it, else the first stage.
pub fn next_stage(stages: &[u32], current: u32) -> Option<u32> {
    if stages.is_empty() {
        return None;
    }
    if let Some(i) = stages.iter().position(|&s| s == current) {
        return Some(stages[(i + 1) % stages.len()]);
    }
    stages.iter().copied().filter(|&s| s > current).min().or(Some(stages[0]))
}

/// The value before `current` in `stages` (wrapping). If `current` is not in
/// the list, the largest stage below it, else the last stage.
pub fn prev_stage(stages: &[u32], current: u32) -> Option<u32> {
    if stages.is_empty() {
        return None;
    }
    if let Some(i) = stages.iter().position(|&s| s == current) {
        return Some(stages[(i + stages.len() - 1) % stages.len()]);
    }
    stages.iter().copied().filter(|&s| s < current).max().or(stages.last().copied())
}

/// "Razer DeathAdder Elite" -> "DeathAdder Elite" (the OSD is small).
pub fn short_name(product: &str) -> String {
    let t = product.trim();
    t.strip_prefix("Razer ").unwrap_or(t).to_string()
}

/// Candidate HID interfaces, one per physical mouse as far as we can tell:
/// the generic mouse collection (usage page 1, usage 2) when there is one.
fn candidates(api: &HidApi) -> Vec<&DeviceInfo> {
    let razer: Vec<&DeviceInfo> = api.device_list().filter(|d| d.vendor_id() == RAZER_VID).collect();
    let mice: Vec<&DeviceInfo> = razer.iter().copied().filter(|d| d.usage_page() == 1 && d.usage() == 2).collect();
    if mice.is_empty() { razer } else { mice }
}

struct Dev {
    path: CString,
    key: String,
    name: String,
    dev: HidDevice,
    last: Option<u32>,
}

fn scan(devs: &mut Vec<Dev>, skipped: &mut HashSet<CString>) {
    let Ok(api) = HidApi::new() else { return };
    let cands = candidates(&api);
    let present: HashSet<&std::ffi::CStr> = cands.iter().map(|d| d.path()).collect();
    skipped.retain(|p| present.contains(p.as_c_str()));
    for d in cands {
        if devs.iter().any(|x| x.path.as_c_str() == d.path()) || skipped.contains(d.path()) {
            continue;
        }
        let opened = d.open_device(&api).ok().filter(|dev| query(dev).is_some());
        match opened {
            Some(dev) => {
                let key = device_key(&d.path().to_string_lossy()).unwrap_or_else(|| format!("vid_{:04x}&pid_{:04x}", d.vendor_id(), d.product_id()));
                let name = d.product_string().map(short_name).filter(|n| !n.is_empty()).unwrap_or_else(|| "Razer mouse".into());
                devs.push(Dev { path: d.path().to_owned(), key, name, dev, last: None });
            }
            None => {
                skipped.insert(d.path().to_owned());
            }
        }
    }
}

/// Spawn the poller. Queues a [`DpiEvent`] and posts `msg` to `hwnd`
/// whenever any mouse's DPI changes (never for the first reading after
/// (re)connecting).
pub fn spawn(hwnd: isize, msg: u32, shared: Arc<Shared>) {
    std::thread::Builder::new()
        .name("dpi".into())
        .spawn(move || {
            let hwnd = HWND(hwnd as *mut _);
            let mut devs: Vec<Dev> = Vec::new();
            let mut skipped: HashSet<CString> = HashSet::new();
            let mut last_scan: Option<Instant> = None;
            loop {
                let ms = shared.poll_ms.load(Ordering::Relaxed);
                if ms == 0 {
                    devs.clear();
                    last_scan = None;
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
                if last_scan.is_none_or(|t| t.elapsed() >= RESCAN) {
                    scan(&mut devs, &mut skipped);
                    last_scan = Some(Instant::now());
                }
                if devs.is_empty() {
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let multi = devs.len() > 1;
                // A mouse that stops answering was unplugged: forget it; the rescan finds it again.
                devs.retain_mut(|d| {
                    let Some(mut now) = query(&d.dev) else { return false };
                    if let Some(prev) = d.last {
                        if prev != now {
                            // A change we made ourselves (a DPI action): already shown, nothing to cycle.
                            let ours = shared.expected.lock().ok().and_then(|mut e| e.remove(&d.key));
                            if ours == Some(now) {
                                d.last = Some(now);
                                return true;
                            }
                            // The DPI button (or other software) changed it.
                            if let Some(target) = next_stage(&shared.stages_for(&d.key), prev) {
                                if target != now && set_dpi(&d.dev, target) {
                                    now = target;
                                }
                            }
                            if let Ok(mut e) = shared.events.lock() {
                                e.push(DpiEvent { name: d.name.clone(), dpi: now, multi, note: None });
                            }
                            unsafe {
                                let _ = PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0));
                            }
                        }
                    }
                    d.last = Some(now);
                    true
                });
                std::thread::sleep(Duration::from_millis(ms.max(100) as u64));
            }
        })
        .expect("spawn dpi thread");
}

// ---------- DPI actions (button -> change DPI) ----------

/// Run one DPI action on the mouse behind `key`. Returns (name, new DPI, several DPI mice connected).
fn run_command(shared: &Shared, key: Option<String>, cmd: DpiCmd) -> Result<(String, u32, bool), String> {
    let capable: Vec<_> = crate::devices::list_mice().into_iter().filter(|m| m.dpi_capable).collect();
    let mouse = match &key {
        Some(k) => capable.iter().find(|m| &m.key == k).ok_or("This mouse has no DPI control")?,
        None if capable.len() == 1 => &capable[0],
        None => return Err("Mouse not identified".into()),
    };
    let stages = {
        let s = shared.stages_for(&mouse.key);
        if s.is_empty() { DEFAULT_STAGES.to_vec() } else { s }
    };
    let pick = |cur: u32| match cmd {
        DpiCmd::Next => next_stage(&stages, cur),
        DpiCmd::Prev => prev_stage(&stages, cur),
        DpiCmd::Set(v) => Some(v),
    };
    let name = short_name(&mouse.name);
    let multi = capable.len() > 1;
    if mouse.key.starts_with("vid_1532") {
        let api = HidApi::new().map_err(|e| e.to_string())?;
        let info = candidates(&api)
            .into_iter()
            .find(|d| device_key(&d.path().to_string_lossy()).as_deref() == Some(mouse.key.as_str()))
            .ok_or("Mouse not found")?;
        let dev = info.open_device(&api).map_err(|e| e.to_string())?;
        let cur = query(&dev).ok_or("Mouse did not answer")?;
        let target = pick(cur).unwrap_or(cur);
        if let Ok(mut e) = shared.expected.lock() {
            e.insert(mouse.key.clone(), target);
        }
        if !set_dpi(&dev, target) {
            if let Ok(mut e) = shared.expected.lock() {
                e.remove(&mouse.key);
            }
            return Err("Could not set DPI".into());
        }
        Ok((name, target, multi))
    } else if mouse.key.starts_with(&format!("vid_{:04x}", logitech::LOGI_VID)) {
        let h = Hidpp::open(&mouse.key).ok_or("Could not talk to the mouse")?;
        let cur = h.dpi().ok_or("Mouse did not answer")?;
        let target = pick(cur).unwrap_or(cur);
        let now = h.set_dpi(target).ok_or("Could not set DPI")?;
        Ok((name, now, multi))
    } else {
        Err("This mouse has no DPI control".into())
    }
}

/// Spawn the thread that carries out DPI actions (HID I/O never runs on the mouse-hook thread).
pub fn spawn_worker(hwnd: isize, msg: u32, shared: Arc<Shared>) {
    let (tx, rx) = mpsc::channel::<(Option<String>, DpiCmd)>();
    if let Ok(mut c) = shared.commands.lock() {
        *c = Some(tx);
    }
    std::thread::Builder::new()
        .name("dpi-actions".into())
        .spawn(move || {
            let hwnd = HWND(hwnd as *mut _);
            while let Ok((key, cmd)) = rx.recv() {
                let ev = match run_command(&shared, key, cmd) {
                    Ok((name, dpi, multi)) => DpiEvent { name, dpi, multi, note: None },
                    Err(e) => DpiEvent { name: String::new(), dpi: 0, multi: false, note: Some(e) },
                };
                if let Ok(mut e) = shared.events.lock() {
                    e.push(ev);
                }
                unsafe {
                    let _ = PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0));
                }
            }
        })
        .expect("spawn dpi action thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previous_stage() {
        let s = [800, 1600, 2400];
        assert_eq!(prev_stage(&s, 1600), Some(800));
        assert_eq!(prev_stage(&s, 800), Some(2400));
        assert_eq!(prev_stage(&s, 2000), Some(1600));
        assert_eq!(prev_stage(&s, 100), Some(2400));
        assert_eq!(prev_stage(&[], 800), None);
    }

    #[test]
    fn cycling() {
        let s = [800, 1600, 2400];
        assert_eq!(next_stage(&s, 800), Some(1600));
        assert_eq!(next_stage(&s, 2400), Some(800));
        assert_eq!(next_stage(&s, 1000), Some(1600)); // not in list
        assert_eq!(next_stage(&s, 9000), Some(800));
        assert_eq!(next_stage(&[], 800), None);
    }

    #[test]
    fn names() {
        assert_eq!(short_name("Razer DeathAdder Elite"), "DeathAdder Elite");
        assert_eq!(short_name("Other"), "Other");
    }
}

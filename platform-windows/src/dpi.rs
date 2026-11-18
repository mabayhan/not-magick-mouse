//! Razer DPI reader/writer. Verified on a DeathAdder Elite (PID 005C): the
//! mouse collection (HID interface 0) answers "get DPI" (class 0x04, command
//! 0x85) and accepts "set DPI" (0x05), transaction id 0x3F. The mouse emits no
//! event on a DPI change and refuses to store a custom stage list (0x06), so
//! we poll, and when the DPI button moves the firmware to a new value we
//! replace it with the next value from the user's own list.

use hidapi::{HidApi, HidDevice};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const RAZER_VID: u16 = 0x1532;
const TXN: u8 = 0x3F;

#[derive(Default)]
pub struct Shared {
    poll_ms: AtomicU32,
    stages: Mutex<Vec<u32>>,
}

impl Shared {
    /// `poll_ms == 0` pauses all DPI handling.
    pub fn configure(&self, poll_ms: u32, stages: &[u32]) {
        self.poll_ms.store(poll_ms, Ordering::Relaxed);
        if let Ok(mut s) = self.stages.lock() {
            *s = stages.to_vec();
        }
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

fn find_device() -> Option<HidDevice> {
    let api = HidApi::new().ok()?;
    let mut infos: Vec<_> = api.device_list().filter(|d| d.vendor_id() == RAZER_VID).collect();
    // The generic mouse collection (usage page 1, usage 2) first.
    infos.sort_by_key(|d| !(d.usage_page() == 1 && d.usage() == 2));
    for d in infos {
        if let Ok(dev) = d.open_device(&api) {
            if query(&dev).is_some() {
                return Some(dev);
            }
        }
    }
    None
}

/// Spawn the poller. Posts `msg` to `hwnd` with the new DPI in `wparam`
/// whenever it changes (never for the first reading after (re)connecting).
pub fn spawn(hwnd: isize, msg: u32, shared: Arc<Shared>) {
    std::thread::Builder::new()
        .name("dpi".into())
        .spawn(move || {
            let hwnd = HWND(hwnd as *mut _);
            loop {
                if shared.poll_ms.load(Ordering::Relaxed) == 0 {
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let Some(dev) = find_device() else {
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                };
                let mut last: Option<u32> = None;
                loop {
                    let ms = shared.poll_ms.load(Ordering::Relaxed);
                    if ms == 0 {
                        std::thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                    let Some(mut now) = query(&dev) else { break }; // unplugged: reopen
                    if let Some(prev) = last {
                        if prev != now {
                            // The DPI button (or other software) changed it.
                            let stages = shared.stages.lock().map(|s| s.clone()).unwrap_or_default();
                            if let Some(target) = next_stage(&stages, prev) {
                                if target != now && set_dpi(&dev, target) {
                                    now = target;
                                }
                            }
                            unsafe {
                                let _ = PostMessageW(Some(hwnd), msg, WPARAM(now as usize), LPARAM(0));
                            }
                        }
                    }
                    last = Some(now);
                    std::thread::sleep(Duration::from_millis(ms.max(100) as u64));
                }
            }
        })
        .expect("spawn dpi thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycling() {
        let s = [800, 1600, 2400];
        assert_eq!(next_stage(&s, 800), Some(1600));
        assert_eq!(next_stage(&s, 2400), Some(800));
        assert_eq!(next_stage(&s, 1000), Some(1600)); // not in list
        assert_eq!(next_stage(&s, 9000), Some(800));
        assert_eq!(next_stage(&[], 800), None);
    }
}

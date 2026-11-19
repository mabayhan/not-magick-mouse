//! Logitech HID++ 2.0: read and set the sensor DPI ("Adjustable DPI", feature
//! 0x2201) through the vendor collection (usage page 0xFF43). Verified on an
//! MX Master 2S over Bluetooth (HID++ 4.5). Other Logitech mice that speak
//! HID++ 2.0 should work the same way.

use hidapi::{HidApi, HidDevice};
use nmm_core::config::device_key;

pub const LOGI_VID: u16 = 0x046d;
pub const VENDOR_PAGE: u16 = 0xff43;
const SW_ID: u8 = 0x0A;
const FEATURE_ADJUSTABLE_DPI: u16 = 0x2201;

/// DPI values a sensor accepts.
#[derive(Debug, Clone, PartialEq)]
pub enum DpiRange {
    Step { min: u32, max: u32, step: u32 },
    List(Vec<u32>),
}

impl DpiRange {
    /// `words` are the big-endian 16-bit values of the sensor's DPI list reply.
    /// A word with the top three bits set (`0xE000 | step`) turns the neighbours into a range.
    pub fn parse(words: &[u16]) -> Option<DpiRange> {
        let w: Vec<u16> = words.iter().copied().take_while(|&v| v != 0).collect();
        if w.len() >= 3 && w[1] & 0xE000 == 0xE000 {
            let step = (w[1] & 0x1FFF) as u32;
            return (step > 0).then_some(DpiRange::Step { min: w[0] as u32, max: w[2] as u32, step });
        }
        (!w.is_empty()).then(|| DpiRange::List(w.iter().map(|&v| v as u32).collect()))
    }

    /// The accepted value closest to `dpi`.
    pub fn snap(&self, dpi: u32) -> u32 {
        match self {
            DpiRange::Step { min, max, step } => {
                let d = dpi.clamp(*min, *max);
                (min + ((d - min) + step / 2) / step * step).min(*max)
            }
            DpiRange::List(l) => l.iter().copied().min_by_key(|v| v.abs_diff(dpi)).unwrap_or(dpi),
        }
    }
}

pub struct Hidpp {
    dev: HidDevice,
    idx: u8,
    feat: u8,
    pub range: Option<DpiRange>,
}

impl Hidpp {
    /// Open the mouse with hardware key `key` (`vid_046d&pid_xxxx`).
    pub fn open(key: &str) -> Option<Hidpp> {
        let api = HidApi::new().ok()?;
        for info in api.device_list() {
            if info.vendor_id() != LOGI_VID || info.usage_page() != VENDOR_PAGE {
                continue;
            }
            if device_key(&info.path().to_string_lossy()).as_deref() != Some(key) {
                continue;
            }
            let Ok(dev) = info.open_device(&api) else { continue };
            // 0xFF: the device itself (Bluetooth / wired). 1..6: a mouse behind a receiver.
            for idx in [0xFFu8, 1, 2, 3, 4, 5, 6] {
                if xfer(&dev, idx, 0x00, 1, &[0, 0, 0x55]).is_none() {
                    continue; // not HID++ 2.0 on this index
                }
                let Some(f) = xfer(&dev, idx, 0x00, 0, &FEATURE_ADJUSTABLE_DPI.to_be_bytes()) else { continue };
                if f[0] == 0 {
                    continue; // no adjustable DPI here
                }
                let mut h = Hidpp { dev, idx, feat: f[0], range: None };
                h.range = h.read_range();
                return Some(h);
            }
        }
        None
    }

    fn call(&self, func: u8, params: &[u8]) -> Option<Vec<u8>> {
        xfer(&self.dev, self.idx, self.feat, func, params)
    }

    fn read_range(&self) -> Option<DpiRange> {
        let r = self.call(1, &[0])?;
        let words: Vec<u16> = r[1..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        DpiRange::parse(&words)
    }

    pub fn dpi(&self) -> Option<u32> {
        let r = self.call(2, &[0])?;
        Some(u16::from_be_bytes([r[1], r[2]]) as u32)
    }

    /// Set the DPI (snapped to a value the sensor accepts) and return what the mouse now reports.
    pub fn set_dpi(&self, dpi: u32) -> Option<u32> {
        let v = self.range.as_ref().map_or(dpi, |r| r.snap(dpi)).min(u16::MAX as u32) as u16;
        let [h, l] = v.to_be_bytes();
        self.call(3, &[0, h, l])?;
        self.dpi()
    }
}

/// One HID++ long-report request/response. Notifications that arrive in between are skipped.
fn xfer(dev: &HidDevice, idx: u8, feat: u8, func: u8, params: &[u8]) -> Option<Vec<u8>> {
    let mut req = [0u8; 20];
    req[0] = 0x11;
    req[1] = idx;
    req[2] = feat;
    req[3] = (func << 4) | SW_ID;
    req[4..4 + params.len()].copy_from_slice(params);
    dev.write(&req).ok()?;
    for _ in 0..16 {
        let mut buf = [0u8; 64];
        let n = dev.read_timeout(&mut buf, 400).ok()?;
        if n == 0 {
            return None;
        }
        let r = &buf[..n];
        if r.len() >= 6 && r[0] == 0x11 && r[2] == 0xFF && r[3] == feat {
            return None; // HID++ error report
        }
        if r.len() >= 5 && r[0] == 0x11 && r[2] == feat && r[3] == ((func << 4) | SW_ID) {
            return Some(r[4..].to_vec());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mx_master_range() {
        // Reply seen from an MX Master 2S: 200 .. 4000 in steps of 50.
        let r = DpiRange::parse(&[0x00c8, 0xe032, 0x0fa0, 0, 0, 0]).unwrap();
        assert_eq!(r, DpiRange::Step { min: 200, max: 4000, step: 50 });
        assert_eq!(r.snap(1000), 1000);
        assert_eq!(r.snap(1020), 1000);
        assert_eq!(r.snap(1030), 1050);
        assert_eq!(r.snap(50), 200);
        assert_eq!(r.snap(9000), 4000);
    }

    #[test]
    fn discrete_list() {
        let r = DpiRange::parse(&[400, 800, 1600, 0]).unwrap();
        assert_eq!(r.snap(1000), 800);
        assert_eq!(r.snap(5000), 1600);
        assert_eq!(DpiRange::parse(&[0, 0]), None);
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// Changes the DPI of the first HID++ mouse and puts it back.
    #[test]
    #[ignore = "talks to a real mouse"]
    fn set_and_restore() {
        let key = crate::devices::list_mice().into_iter().find(|m| m.key.starts_with("vid_046d") && m.dpi_capable).expect("no Logitech HID++ mouse").key;
        let h = Hidpp::open(&key).expect("open");
        let orig = h.dpi().unwrap();
        println!("range {:?}, original {orig}", h.range);
        let target = if orig == 1200 { 1600 } else { 1200 };
        let now = h.set_dpi(target).expect("set");
        println!("set {target} -> mouse reports {now}");
        let back = h.set_dpi(orig).expect("restore");
        println!("restored -> {back}");
        assert_eq!(now, target);
        assert_eq!(back, orig);
    }
}

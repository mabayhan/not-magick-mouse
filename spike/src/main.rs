//! Phase 0 spike: find out how to read DPI from a Razer mouse.
//!
//! 1. Lists every Razer (VID 1532) HID interface.
//! 2. Tries the "get DPI" Razer report on each, with several transaction ids.
//! 3. For 15 s, dumps input reports from every interface so you can press the
//!    DPI button and see whether the mouse reports it, while also re-polling DPI.

use hidapi::{HidApi, HidDevice};
use std::time::{Duration, Instant};

const RAZER_VID: u16 = 0x1532;

fn razer_report(txn: u8, class: u8, cmd: u8, size: u8, args: &[u8]) -> [u8; 91] {
    // [0]=HID report id, then the 90-byte Razer packet.
    let mut b = [0u8; 91];
    b[2] = txn;
    b[6] = size;
    b[7] = class;
    b[8] = cmd;
    b[9..9 + args.len()].copy_from_slice(args);
    let mut crc = 0u8;
    for x in &b[3..89] {
        crc ^= x;
    }
    b[89] = crc;
    b
}

fn query_dpi(dev: &HidDevice, txn: u8) -> Result<(u16, u16, u8), String> {
    let req = razer_report(txn, 0x04, 0x85, 0x07, &[0x01]);
    dev.send_feature_report(&req).map_err(|e| format!("send: {e}"))?;
    std::thread::sleep(Duration::from_millis(30));
    let mut resp = [0u8; 91];
    let n = dev.get_feature_report(&mut resp).map_err(|e| format!("get: {e}"))?;
    if n < 12 {
        return Err(format!("short reply ({n} bytes)"));
    }
    let status = resp[1];
    let x = u16::from_be_bytes([resp[10], resp[11]]);
    let y = u16::from_be_bytes([resp[12], resp[13]]);
    Ok((x, y, status))
}

fn main() {
    let api = HidApi::new().expect("hidapi init");
    let infos: Vec<_> = api.device_list().filter(|d| d.vendor_id() == RAZER_VID).collect();
    if infos.is_empty() {
        println!("No Razer HID devices found (VID 1532).");
        return;
    }
    println!("Razer HID interfaces:");
    for d in &infos {
        println!(
            "  pid={:04x} iface={} usage_page={:04x} usage={:04x} product={:?} path={:?}",
            d.product_id(), d.interface_number(), d.usage_page(), d.usage(),
            d.product_string().unwrap_or("?"), d.path()
        );
    }

    let mut devs: Vec<(String, HidDevice)> = Vec::new();
    for d in &infos {
        match d.open_device(&api) {
            Ok(dev) => devs.push((format!("pid={:04x} iface={} up={:04x}", d.product_id(), d.interface_number(), d.usage_page()), dev)),
            Err(e) => println!("  cannot open iface {}: {e}", d.interface_number()),
        }
    }

    println!("\nTrying 'get DPI' (class 04, cmd 85) on each interface:");
    let mut working: Vec<(usize, u8)> = Vec::new();
    for (i, (name, dev)) in devs.iter().enumerate() {
        for txn in [0x3f, 0xff, 0x1f, 0x9f] {
            match query_dpi(dev, txn) {
                Ok((x, y, status)) => {
                    println!("  [{name}] txn={txn:02x}: status={status:02x} dpi=({x}, {y})");
                    if x >= 100 && x <= 40000 && status == 0x02 {
                        working.push((i, txn));
                    }
                }
                Err(e) => println!("  [{name}] txn={txn:02x}: {e}"),
            }
        }
    }
    if working.is_empty() {
        println!("\nNo interface answered with status 0x02 and a plausible DPI.");
    } else {
        println!("\nWorking combos (iface index, txn): {working:?}");
    }

    println!("\nPress the DPI button now. Watching 15 s for input reports and DPI changes...");
    for (_, dev) in &devs {
        let _ = dev.set_blocking_mode(false);
    }
    let start = Instant::now();
    let mut last: Option<u16> = None;
    while start.elapsed() < Duration::from_secs(15) {
        for (name, dev) in &devs {
            let mut buf = [0u8; 64];
            if let Ok(n) = dev.read_timeout(&mut buf, 5) {
                if n > 0 {
                    // Mouse movement reports are noisy; show only non-movement-looking ones.
                    println!("  [{name}] input report ({n}): {:02x?}", &buf[..n]);
                }
            }
        }
        if let Some(&(i, txn)) = working.first() {
            if let Ok((x, _, _)) = query_dpi(&devs[i].1, txn) {
                if last != Some(x) {
                    println!("  DPI now {x} (t={:.1}s)", start.elapsed().as_secs_f32());
                    last = Some(x);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    println!("Done.");
}

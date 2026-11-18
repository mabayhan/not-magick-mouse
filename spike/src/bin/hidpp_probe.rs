//! READ-ONLY probe of Logitech HID++ 2.0 on the vendor collection: protocol
//! ping, find the "Adjustable DPI" feature (0x2201) and read the current DPI.
//! `cargo run -p nmm-spike --bin hidpp_probe`  (quit Logitech Options+ first)
use std::time::Duration;

const SW: u8 = 0x0A;

fn xfer(dev: &hidapi::HidDevice, idx: u8, feat: u8, func: u8, params: &[u8]) -> Result<Vec<u8>, String> {
    let mut req = [0u8; 20];
    req[0] = 0x11;
    req[1] = idx;
    req[2] = feat;
    req[3] = (func << 4) | SW;
    req[4..4 + params.len()].copy_from_slice(params);
    dev.write(&req).map_err(|e| format!("write: {e}"))?;
    for _ in 0..20 {
        let mut buf = [0u8; 64];
        let n = dev.read_timeout(&mut buf, 500).map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            return Err("timeout".into());
        }
        let r = &buf[..n];
        // HID++ error report: 11 idx 8F feat func err
        if r.len() >= 6 && r[2] == 0xFF && r[3] == feat {
            return Err(format!("device error code {:#04x}", r[5]));
        }
        if r.len() >= 4 && r[0] == 0x11 && r[2] == feat && r[3] == ((func << 4) | SW) {
            return Ok(r[4..].to_vec());
        }
    }
    Err("no matching reply".into())
}

fn main() {
    let api = hidapi::HidApi::new().unwrap();
    let Some(info) = api.device_list().find(|d| d.vendor_id() == 0x046d && d.usage_page() == 0xff43) else {
        println!("no Logitech vendor collection (usage page ff43) found");
        return;
    };
    println!("opening {} ({:?})", info.path().to_string_lossy(), info.product_string());
    let dev = match info.open_device(&api) {
        Ok(d) => d,
        Err(e) => return println!("open failed: {e}"),
    };
    let _ = Duration::from_millis(1);
    for idx in [0xFFu8, 0x01, 0x00] {
        print!("device index {idx:#04x}: ");
        match xfer(&dev, idx, 0x00, 1, &[0, 0, 0x55]) {
            Ok(r) => {
                println!("HID++ {}.{} (ping echo {:#04x})", r[0], r[1], r[2]);
                match xfer(&dev, idx, 0x00, 0, &[0x22, 0x01]) {
                    Ok(f) if f[0] != 0 => {
                        let fi = f[0];
                        println!("  Adjustable DPI feature index = {fi}");
                        if let Ok(l) = xfer(&dev, idx, fi, 1, &[0]) {
                            println!("  sensor dpi list (raw) = {:02x?}", &l[..12.min(l.len())]);
                        }
                        match xfer(&dev, idx, fi, 2, &[0]) {
                            Ok(d) => println!("  CURRENT DPI = {} (default {})", u16::from_be_bytes([d[1], d[2]]), u16::from_be_bytes([d[3], d[4]])),
                            Err(e) => println!("  get dpi failed: {e}"),
                        }
                    }
                    Ok(_) => println!("  feature 0x2201 not supported"),
                    Err(e) => println!("  feature lookup failed: {e}"),
                }
                return;
            }
            Err(e) => println!("{e}"),
        }
    }
}

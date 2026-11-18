//! Probe which DPI write commands the mouse accepts. Restores the original DPI.
use hidapi::{HidApi, HidDevice};
use std::time::Duration;

fn xfer(dev: &HidDevice, class: u8, cmd: u8, size: u8, args: &[u8]) -> Vec<u8> {
    let mut b = [0u8; 91];
    b[2] = 0x3f;
    b[6] = size;
    b[7] = class;
    b[8] = cmd;
    b[9..9 + args.len()].copy_from_slice(args);
    let mut crc = 0u8;
    for x in &b[3..89] {
        crc ^= x;
    }
    b[89] = crc;
    dev.send_feature_report(&b).unwrap();
    std::thread::sleep(Duration::from_millis(40));
    let mut r = [0u8; 91];
    dev.get_feature_report(&mut r).unwrap();
    r.to_vec()
}

fn get_dpi(dev: &HidDevice) -> (u8, u16) {
    let r = xfer(dev, 4, 0x85, 7, &[1]);
    (r[1], u16::from_be_bytes([r[10], r[11]]))
}

fn set_dpi(dev: &HidDevice, d: u16) -> u8 {
    let [h, l] = d.to_be_bytes();
    xfer(dev, 4, 0x05, 7, &[1, h, l, h, l, 0, 0])[1]
}

fn main() {
    let api = HidApi::new().unwrap();
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == 0x1532 && d.usage_page() == 1 && d.usage() == 2)
        .expect("no mouse");
    let dev = info.open_device(&api).unwrap();
    let (st, orig) = get_dpi(&dev);
    println!("original: status={st:02x} dpi={orig}");

    for target in [1800u16, 3200] {
        let st = set_dpi(&dev, target);
        std::thread::sleep(Duration::from_millis(100));
        println!("set_dpi({target}): status={st:02x}, read back {:?}", get_dpi(&dev));
    }

    // Stages: [varstore, active, count, (idx, x, x, y, y, 0, 0)*count]
    let stages = [800u16, 1600, 2400];
    let mut args = vec![0x00u8, 0, stages.len() as u8];
    for (i, s) in stages.iter().enumerate() {
        let [h, l] = s.to_be_bytes();
        args.extend_from_slice(&[i as u8, h, l, h, l, 0, 0]);
    }
    let r = xfer(&dev, 4, 0x06, args.len() as u8, &args);
    println!("set_stages (varstore 0, nostore): status={:02x}", r[1]);
    let r = xfer(&dev, 4, 0x86, 0x26, &[0]);
    println!("get_stages: status={:02x} active={} count={} data={:02x?}", r[1], r[10], r[11], &r[10..40]);

    println!("restore: set_dpi status={:02x}", set_dpi(&dev, orig));
    println!("now: {:?}", get_dpi(&dev));
}

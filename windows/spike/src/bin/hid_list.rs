//! Lists HID collections of one vendor: `cargo run -p nmm-spike --bin hid_list -- 046d`
fn main() {
    let vid = u16::from_str_radix(&std::env::args().nth(1).unwrap_or("046d".into()), 16).unwrap();
    let api = hidapi::HidApi::new().unwrap();
    for d in api.device_list().filter(|d| d.vendor_id() == vid) {
        println!(
            "pid={:04x} usage_page={:04x} usage={:04x} iface={} product={:?} path={}",
            d.product_id(), d.usage_page(), d.usage(), d.interface_number(), d.product_string(), d.path().to_string_lossy()
        );
    }
}

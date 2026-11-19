#[cfg(target_os = "linux")]
fn main() {
    use nmm_core::config::explicit_config_path;
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("nmmd [--config <path>]\n\nnot-magick-mouse daemon. Config: ~/.config/not-magick-mouse/config.toml (or NMM_CONFIG).");
        return;
    }
    if let Err(e) = nmm_platform_linux::run(explicit_config_path(args)) {
        eprintln!("nmmd: {e:#}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("nmmd only runs on Linux");
}

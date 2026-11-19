//! The daemon: finds mice, grabs them, follows focus, reloads the config.

use crate::control::Control;
use crate::focus;
use crate::ipc::lock_instance;
use crate::mouse::{Shared, is_mouse, is_ours, run_device};
use crate::output::Keyboard;
use crate::tray;
use anyhow::{Context, Result};
use evdev::Device;
use nmm_core::config::{Config, default_config_path, saved_config_path};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};

pub(crate) fn cfg_path(explicit: &Option<PathBuf>) -> PathBuf {
    explicit.clone().or_else(saved_config_path).unwrap_or_else(default_config_path)
}

/// Only one daemon may grab the mice. The lock lives as long as the returned file.
fn single_instance() -> Result<std::fs::File> {
    lock_instance("nmmd").ok_or_else(|| anyhow::anyhow!("another nmmd is already running"))
}

fn mtime(p: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Run until killed (the grabs are released when the process exits).
pub fn run(explicit_config: Option<PathBuf>) -> Result<()> {
    let _lock = single_instance()?;
    let path = cfg_path(&explicit_config);
    let cfg = Config::load(&path).unwrap_or_else(|e| {
        eprintln!("nmmd: config error, using defaults: {e:#}");
        Config::default()
    });
    let exe = Arc::new(Mutex::new(None));
    focus::spawn(exe.clone());
    let kbd = Keyboard::new().context("virtual keyboard")?;
    let ctl = Arc::new(Control::new());
    let shared = Arc::new(Shared { cfg: RwLock::new(cfg), exe, kbd, ctl: ctl.clone() });
    tray::spawn(ctl.clone(), explicit_config.clone());
    eprintln!("nmmd: config {}", path.display());

    // Reload when the file (or the path chosen in the GUI) changes. A broken file keeps the last good config.
    {
        let shared = shared.clone();
        let explicit = explicit_config.clone();
        std::thread::spawn(move || {
            let mut last = (cfg_path(&explicit), mtime(&cfg_path(&explicit)));
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let p = cfg_path(&explicit);
                let now = (p.clone(), mtime(&p));
                let forced = shared.ctl.reload.swap(false, Ordering::Relaxed);
                if now == last && !forced {
                    continue;
                }
                last = now;
                match Config::load(&p) {
                    Ok(c) => {
                        if let Ok(mut g) = shared.cfg.write() {
                            *g = c;
                        }
                        eprintln!("nmmd: reloaded {}", p.display());
                    }
                    Err(e) => eprintln!("nmmd: config error (keeping previous): {e:#}"),
                }
            }
        });
    }

    // Hot-plug: scan /dev/input for new pointers.
    let active: Arc<Mutex<HashSet<PathBuf>>> = Arc::default();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    loop {
        let mut present = HashSet::new();
        if let Ok(rd) = std::fs::read_dir("/dev/input") {
            for e in rd.flatten() {
                let p = e.path();
                if !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("event")) {
                    continue;
                }
                present.insert(p.clone());
                if seen.contains(&p) {
                    continue;
                }
                let Ok(dev) = Device::open(&p) else { continue }; // permissions: not added to `seen`, retried
                seen.insert(p.clone());
                if is_ours(&dev) || !is_mouse(&dev) {
                    continue;
                }
                drop(dev);
                if let Ok(mut a) = active.lock() {
                    a.insert(p.clone());
                }
                let (shared, active) = (shared.clone(), active.clone());
                std::thread::spawn(move || {
                    if let Err(e) = run_device(&p, &shared) {
                        eprintln!("nmmd: {}: {e:#}", p.display());
                    }
                    if let Ok(mut a) = active.lock() {
                        a.remove(&p);
                    }
                });
            }
        }
        // Forget unplugged nodes so a re-plugged mouse is picked up again.
        seen.retain(|p| present.contains(p));
        std::thread::sleep(Duration::from_secs(2));
    }
}

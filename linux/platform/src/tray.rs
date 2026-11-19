//! Tray icon (StatusNotifierItem over D-Bus), the Linux counterpart of the Windows tray.
//! KDE, Xfce, Cinnamon, MATE, LXQt and others show it natively; GNOME needs the
//! "AppIndicator and KStatusNotifierItem Support" extension. Without a tray host
//! the daemon simply keeps remapping, headless.

use crate::control::Control;
use crate::daemon::cfg_path;
use ksni::MenuItem;
use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, StandardItem};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::{Arc, LazyLock};

const ICON_PNG: &[u8] = include_bytes!("../../../assets/icon256.png");

static ICON: LazyLock<Option<ksni::Icon>> = LazyLock::new(|| {
    let img = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png).ok()?.into_rgba8();
    let (width, height) = (img.width() as i32, img.height() as i32);
    let mut data = img.into_vec();
    for px in data.as_chunks_mut::<4>().0 {
        px.rotate_right(1); // RGBA -> ARGB
    }
    Some(ksni::Icon { width, height, data })
});

struct NmmTray {
    ctl: Arc<Control>,
    explicit: Option<PathBuf>,
}

/// Starts the GUI that sits next to this binary (or in PATH) and reaps it when it exits.
fn open_settings(explicit: &Option<PathBuf>) {
    let beside = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("nmm-gui"))).filter(|p| p.exists());
    let mut cmd = Command::new(beside.unwrap_or_else(|| PathBuf::from("nmm-gui")));
    if let Some(p) = explicit {
        cmd.arg("--config").arg(p);
    }
    match cmd.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => eprintln!("nmmd: cannot start nmm-gui: {e}"),
    }
}

fn open_config_folder(explicit: &Option<PathBuf>) {
    if let Some(dir) = cfg_path(explicit).parent() {
        let _ = std::fs::create_dir_all(dir);
        let _ = Command::new("xdg-open").arg(dir).spawn().map(|mut c| {
            std::thread::spawn(move || {
                let _ = c.wait();
            });
        });
    }
}

impl ksni::Tray for NmmTray {
    fn id(&self) -> String {
        "not-magick-mouse".into()
    }

    fn title(&self) -> String {
        "not-magick-mouse".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        ICON.iter().cloned().collect()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        open_settings(&self.explicit);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            CheckmarkItem {
                label: "Enabled".into(),
                checked: self.ctl.enabled.load(Ordering::Relaxed),
                activate: Box::new(|t: &mut Self| {
                    t.ctl.enabled.fetch_xor(true, Ordering::Relaxed);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Settings...".into(),
                activate: Box::new(|t: &mut Self| open_settings(&t.explicit)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Reload config".into(),
                activate: Box::new(|t: &mut Self| t.ctl.reload.store(true, Ordering::Relaxed)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Open config folder".into(),
                activate: Box::new(|t: &mut Self| open_config_folder(&t.explicit)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|_| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Shows the tray icon. The icon appears whenever a tray host is (or becomes) available.
pub fn spawn(ctl: Arc<Control>, explicit: Option<PathBuf>) {
    match (NmmTray { ctl, explicit }).assume_sni_available(true).spawn() {
        Ok(handle) => std::mem::forget(handle), // keep the tray alive for the life of the daemon
        Err(e) => eprintln!("nmmd: no tray icon ({e}); running headless"),
    }
}

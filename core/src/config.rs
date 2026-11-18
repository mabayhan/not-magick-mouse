use crate::action::Action;
use crate::button::Button;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub osd_ms: u32,
    pub osd_position: String,
    /// 0 disables polling-based DPI detection.
    pub dpi_poll_ms: u32,
    /// Custom DPI values the DPI button cycles through. Empty keeps the mouse's own stages.
    pub dpi_stages: Vec<u32>,
    pub start_with_windows: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { osd_ms: 1500, osd_position: "bottom-center".into(), dpi_poll_ms: 500, dpi_stages: Vec::new(), start_with_windows: false }
    }
}

/// Per-button actions. Unset buttons are passthrough.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ButtonMap {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<Action>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<Action>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub middle: Option<Action>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x1: Option<Action>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x2: Option<Action>,
}

impl ButtonMap {
    pub fn get(&self, b: Button) -> Option<&Action> {
        match b {
            Button::Left => &self.left,
            Button::Right => &self.right,
            Button::Middle => &self.middle,
            Button::X1 => &self.x1,
            Button::X2 => &self.x2,
        }
        .as_ref()
    }

    /// Set a button; `Passthrough` clears it.
    pub fn set(&mut self, b: Button, a: Action) {
        let v = if a == Action::Passthrough { None } else { Some(a) };
        match b {
            Button::Left => self.left = v,
            Button::Right => self.right = v,
            Button::Middle => self.middle = v,
            Button::X1 => self.x1 = v,
            Button::X2 => self.x2 = v,
        }
    }
}

/// Buttons of one mouse inside one app (`[[app.device]]`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppDeviceProfile {
    /// Hardware key of the mouse, see [`device_key`].
    pub id: String,
    /// Leave this mouse alone while this app is focused.
    #[serde(default)]
    pub disabled: bool,
    #[serde(flatten)]
    pub buttons: ButtonMap,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppProfile {
    pub exe: String,
    /// Do nothing at all while this app is focused.
    #[serde(default)]
    pub disabled: bool,
    #[serde(flatten)]
    pub buttons: ButtonMap,
    /// Per-mouse overrides inside this app.
    #[serde(default, rename = "device", skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<AppDeviceProfile>,
}

impl AppProfile {
    pub fn device(&self, id: &str) -> Option<&AppDeviceProfile> {
        self.devices.iter().find(|d| d.id.eq_ignore_ascii_case(id))
    }
}

/// Settings for one kind of mouse, matched by its `vid_xxxx&pid_xxxx` key
/// (see [`device_key`]). Two identical mice share one profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceProfile {
    /// Lower-case hardware key, e.g. `vid_1532&pid_005c`.
    pub id: String,
    /// Friendly name, only for display.
    #[serde(default)]
    pub name: String,
    /// Leave this mouse completely alone (all buttons pass through).
    #[serde(default)]
    pub disabled: bool,
    /// DPI values this mouse's DPI button cycles through. Empty falls back to `settings.dpi_stages`.
    #[serde(default)]
    pub dpi_stages: Vec<u32>,
    #[serde(flatten)]
    pub buttons: ButtonMap,
}

/// Normalised hardware key for a raw input / HID device path or instance id.
/// `HID#VID_1532&PID_005C&MI_00#..` -> `vid_1532&pid_005c`;
/// Bluetooth `..._DEV_VID&02046D_PID&B019_REV..` -> `vid_046d&pid_b019`;
/// `HID#VEN_06CB&COL01#..` -> `ven_06cb`. `None` when nothing recognisable.
pub fn device_key(path: &str) -> Option<String> {
    let up = path.to_ascii_uppercase();
    // "VID_1532" (USB) or "VID&02046D" (Bluetooth; two-digit source prefix).
    let field = |tag: &str| -> Option<String> {
        let i = [format!("{tag}_"), format!("{tag}&")].iter().find_map(|t| up.find(t.as_str()).map(|i| i + t.len()))?;
        let h: String = up[i..].chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        (h.len() >= 4).then(|| h[h.len() - 4..].to_ascii_lowercase())
    };
    if let (Some(v), Some(p)) = (field("VID"), field("PID")) {
        return Some(format!("vid_{v}&pid_{p}"));
    }
    field("VEN").map(|v| format!("ven_{v}"))
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub settings: Settings,
    pub global: ButtonMap,
    #[serde(rename = "device", skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<DeviceProfile>,
    #[serde(rename = "app", skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<AppProfile>,
}

impl Config {
    pub fn parse(text: &str) -> Result<Config> {
        let cfg: Config = toml::from_str(text).context("invalid config")?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn validate(&self) -> Result<()> {
        for d in &self.settings.dpi_stages {
            anyhow::ensure!((100..=30000).contains(d), "DPI value {d} is out of range (100-30000)");
        }
        for (i, a) in self.apps.iter().enumerate() {
            anyhow::ensure!(!a.exe.trim().is_empty(), "app profile #{} has an empty exe name", i + 1);
        }
        for (i, d) in self.devices.iter().enumerate() {
            anyhow::ensure!(!d.id.trim().is_empty(), "mouse profile #{} has an empty id", i + 1);
            for v in &d.dpi_stages {
                anyhow::ensure!((100..=30000).contains(v), "DPI value {v} for mouse '{}' is out of range (100-30000)", d.id);
            }
        }
        Ok(())
    }

    /// Non-fatal problems to show in the GUI.
    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (i, a) in self.apps.iter().enumerate() {
            if self.apps[..i].iter().any(|b| b.exe.eq_ignore_ascii_case(&a.exe)) {
                out.push(format!("more than one profile matches '{}'; the first wins", a.exe));
            }
        }
        for (i, d) in self.devices.iter().enumerate() {
            if self.devices[..i].iter().any(|b| b.id.eq_ignore_ascii_case(&d.id)) {
                out.push(format!("more than one profile matches mouse '{}'; the first wins", d.id));
            }
        }
        out
    }

    pub fn device(&self, id: &str) -> Option<&DeviceProfile> {
        self.devices.iter().find(|d| d.id.eq_ignore_ascii_case(id))
    }

    /// DPI list for a mouse: its own, else the global one (empty = keep the mouse's stages).
    pub fn dpi_stages_for(&self, id: &str) -> &[u32] {
        match self.device(id) {
            Some(d) if !d.dpi_stages.is_empty() => &d.dpi_stages,
            _ => &self.settings.dpi_stages,
        }
    }

    /// Missing file means the empty default config (nothing remapped).
    pub fn load(path: &Path) -> Result<Config> {
        match std::fs::read_to_string(path) {
            Ok(t) => Config::parse(&t),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Validate, then write atomically (temp file + rename).
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let text = self.to_toml()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// Default location: `<home>/.config/not-magick-mouse/config.toml`
/// (`%USERPROFILE%` on Windows, `$HOME` elsewhere).
pub fn default_config_path() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".config").join("not-magick-mouse").join("config.toml")
}

/// Where earlier versions kept the file (`%APPDATA%\not-magick-mouse\config.toml`).
fn legacy_config_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("not-magick-mouse").join("config.toml"))
}

/// `--config <path>` / `--config=<path>` on the command line, else the
/// `NMM_CONFIG` environment variable. These override everything else.
pub fn explicit_config_path(args: impl IntoIterator<Item = String>) -> Option<PathBuf> {
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--config" {
            if let Some(p) = it.next() {
                return Some(PathBuf::from(p));
            }
        } else if let Some(p) = a.strip_prefix("--config=") {
            return Some(PathBuf::from(p));
        }
    }
    std::env::var_os("NMM_CONFIG").filter(|p| !p.is_empty()).map(PathBuf::from)
}

/// File that stores a path chosen in the GUI:
/// `<home>/.config/not-magick-mouse/location.txt`.
fn pointer_file() -> PathBuf {
    default_config_path().with_file_name("location.txt")
}

fn read_pointer_at(file: &Path) -> Option<PathBuf> {
    let t = std::fs::read_to_string(file).ok()?;
    let t = t.trim();
    (!t.is_empty()).then(|| PathBuf::from(t))
}

fn write_pointer_at(file: &Path, target: Option<&Path>) -> Result<()> {
    match target {
        Some(p) => {
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(file, p.to_string_lossy().as_bytes())?;
        }
        None => match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        },
    }
    Ok(())
}

/// The config path chosen in the GUI, if any.
pub fn saved_config_path() -> Option<PathBuf> {
    read_pointer_at(&pointer_file())
}

/// Remember `path` as the config location (`None` returns to the default).
pub fn set_saved_config_path(path: Option<&Path>) -> Result<()> {
    write_pointer_at(&pointer_file(), path.filter(|p| *p != default_config_path()))
}

/// Pick the config file. Priority: command line / `NMM_CONFIG`
/// ([`explicit_config_path`]), then the path saved from the GUI, then the
/// default. When the default is used and does not exist yet but a file from
/// the old location does, that file is copied over once.
pub fn resolve_config_path(args: impl IntoIterator<Item = String>) -> PathBuf {
    if let Some(p) = explicit_config_path(args).or_else(saved_config_path) {
        return p;
    }
    let path = default_config_path();
    if !path.exists() {
        if let Some(old) = legacy_config_path().filter(|o| o.exists()) {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::copy(old, &path);
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_default() {
        let c = Config::parse("").unwrap();
        assert_eq!(c, Config::default());
        for b in Button::ALL {
            assert!(c.global.get(b).is_none());
        }
    }

    #[test]
    fn parse_full() {
        let c = Config::parse(
            r#"
[settings]
osd_ms = 900
[global]
x1 = "key:ctrl+w"
[[app]]
exe = "WindowsTerminal.exe"
x1 = "key:ctrl+shift+w"
[[app]]
exe = "game.exe"
disabled = true
"#,
        )
        .unwrap();
        assert_eq!(c.settings.osd_ms, 900);
        assert_eq!(c.apps.len(), 2);
        assert!(c.apps[1].disabled);
        let again = Config::parse(&c.to_toml().unwrap()).unwrap();
        assert_eq!(c, again);
    }

    #[test]
    fn bad_input_rejected() {
        assert!(Config::parse("[global]\nx1 = \"key:nope+\"").is_err());
        assert!(Config::parse("[global]\nbogus = \"none\"").is_err());
        assert!(Config::parse("[[app]]\nexe = \"\"").is_err());
    }

    #[test]
    fn config_flag_wins() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(resolve_config_path(a(&["--config", "C:\\x\\c.toml"])), PathBuf::from("C:\\x\\c.toml"));
        assert_eq!(resolve_config_path(a(&["--config=D:\\y.toml"])), PathBuf::from("D:\\y.toml"));
        assert!(default_config_path().ends_with(".config/not-magick-mouse/config.toml"));
    }

    #[test]
    fn pointer_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("nmm-ptr-{}", std::process::id()));
        let f = dir.join("location.txt");
        assert_eq!(read_pointer_at(&f), None);
        write_pointer_at(&f, Some(Path::new("D:\\x\\c.toml"))).unwrap();
        assert_eq!(read_pointer_at(&f), Some(PathBuf::from("D:\\x\\c.toml")));
        write_pointer_at(&f, None).unwrap();
        assert_eq!(read_pointer_at(&f), None);
        write_pointer_at(&f, None).unwrap(); // removing twice is fine
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn dpi_stages() {
        let c = Config::parse("[settings]\ndpi_stages = [800, 2400]").unwrap();
        assert_eq!(c.settings.dpi_stages, vec![800, 2400]);
        assert!(Config::parse("[settings]\ndpi_stages = [5]").is_err());
    }

    #[test]
    fn device_keys() {
        assert_eq!(device_key(r"\\?\HID#VID_1532&PID_005C&MI_00#8&23f72b55&0&0000#{378de44c}").as_deref(), Some("vid_1532&pid_005c"));
        assert_eq!(
            device_key(r"\\?\HID#{00001812-0000-1000-8000-00805F9B34FB}_DEV_VID&02046D_PID&B019_REV&0006_F2423DEAEA8E&COL02#9&2D68").as_deref(),
            Some("vid_046d&pid_b019")
        );
        assert_eq!(device_key(r"\\?\HID#VEN_06CB&COL01#5&99af808").as_deref(), Some("ven_06cb"));
        assert_eq!(device_key("garbage"), None);
    }

    #[test]
    fn device_profiles() {
        let c = Config::parse(
            "[settings]\ndpi_stages=[800]\n[[device]]\nid=\"vid_1532&pid_005c\"\ndpi_stages=[400,1600]\nx1=\"key:ctrl+w\"\n[[device]]\nid=\"vid_045e&pid_07a5\"",
        )
        .unwrap();
        assert_eq!(c.dpi_stages_for("VID_1532&PID_005C"), &[400, 1600]);
        assert_eq!(c.dpi_stages_for("vid_045e&pid_07a5"), &[800]);
        assert_eq!(c.dpi_stages_for("other"), &[800]);
        assert_eq!(Config::parse(&c.to_toml().unwrap()).unwrap(), c);
        assert!(Config::parse("[[device]]\nid=\"\"").is_err());
    }

    #[test]
    fn duplicate_warning() {
        let c = Config::parse("[[app]]\nexe=\"a.exe\"\n[[app]]\nexe=\"A.EXE\"").unwrap();
        assert_eq!(c.warnings().len(), 1);
    }

    #[test]
    fn save_and_load() {
        let dir = std::env::temp_dir().join(format!("nmm-test-{}", std::process::id()));
        let p = dir.join("config.toml");
        let mut c = Config::default();
        c.global.set(Button::X2, "preset:close_tab".parse().unwrap());
        c.save(&p).unwrap();
        assert_eq!(Config::load(&p).unwrap(), c);
        std::fs::remove_dir_all(dir).ok();
        assert_eq!(Config::load(&p).unwrap(), Config::default());
    }
}

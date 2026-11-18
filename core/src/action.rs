use crate::button::Button;
use crate::keys::{Modifier, is_valid_key};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// Modifiers plus exactly one key, e.g. `ctrl+shift+w`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combo {
    pub mods: Vec<Modifier>,
    pub key: String,
}

impl Combo {
    pub fn new(mods: &[Modifier], key: &str) -> Combo {
        let mut mods = mods.to_vec();
        mods.sort();
        mods.dedup();
        Combo { mods, key: key.to_ascii_lowercase() }
    }
}

impl FromStr for Combo {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let (key, mods) = parts.split_last().ok_or("empty shortcut")?;
        if key.is_empty() {
            return Err("shortcut is missing its final key".into());
        }
        let mut out = Vec::new();
        for m in mods {
            out.push(Modifier::parse(m).ok_or_else(|| format!("unknown modifier '{m}'"))?);
        }
        if !is_valid_key(key) {
            return Err(format!("unknown key '{key}'"));
        }
        Ok(Combo::new(&out, key))
    }
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for m in &self.mods {
            write!(f, "{}+", m.name())?;
        }
        f.write_str(&self.key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    CloseTab,
    CloseTabMdi,
    CloseWindow,
    BrowserBack,
    BrowserForward,
    PlayPause,
    NextTrack,
    PrevTrack,
    Mute,
    VolumeUp,
    VolumeDown,
    DesktopLeft,
    DesktopRight,
    TaskView,
    Copy,
    Paste,
    Undo,
    Redo,
}

/// What a preset resolves to at injection time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetEffect {
    Key(Combo),
    AppCommand(AppCommand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    BrowserBack,
    BrowserForward,
}

impl Preset {
    pub const ALL: [Preset; 18] = [
        Preset::CloseTab, Preset::CloseTabMdi, Preset::CloseWindow,
        Preset::BrowserBack, Preset::BrowserForward,
        Preset::PlayPause, Preset::NextTrack, Preset::PrevTrack,
        Preset::Mute, Preset::VolumeUp, Preset::VolumeDown,
        Preset::DesktopLeft, Preset::DesktopRight, Preset::TaskView,
        Preset::Copy, Preset::Paste, Preset::Undo, Preset::Redo,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Preset::CloseTab => "close_tab",
            Preset::CloseTabMdi => "close_tab_mdi",
            Preset::CloseWindow => "close_window",
            Preset::BrowserBack => "browser_back",
            Preset::BrowserForward => "browser_forward",
            Preset::PlayPause => "play_pause",
            Preset::NextTrack => "next_track",
            Preset::PrevTrack => "prev_track",
            Preset::Mute => "mute",
            Preset::VolumeUp => "volume_up",
            Preset::VolumeDown => "volume_down",
            Preset::DesktopLeft => "desktop_left",
            Preset::DesktopRight => "desktop_right",
            Preset::TaskView => "task_view",
            Preset::Copy => "copy",
            Preset::Paste => "paste",
            Preset::Undo => "undo",
            Preset::Redo => "redo",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::CloseTab => "Close tab / document (Ctrl+W)",
            Preset::CloseTabMdi => "Close tab, MDI (Ctrl+F4)",
            Preset::CloseWindow => "Close window (Alt+F4)",
            Preset::BrowserBack => "Browser back",
            Preset::BrowserForward => "Browser forward",
            Preset::PlayPause => "Play / pause",
            Preset::NextTrack => "Next track",
            Preset::PrevTrack => "Previous track",
            Preset::Mute => "Mute",
            Preset::VolumeUp => "Volume up",
            Preset::VolumeDown => "Volume down",
            Preset::DesktopLeft => "Virtual desktop left",
            Preset::DesktopRight => "Virtual desktop right",
            Preset::TaskView => "Task view (Win+Tab)",
            Preset::Copy => "Copy",
            Preset::Paste => "Paste",
            Preset::Undo => "Undo",
            Preset::Redo => "Redo",
        }
    }

    pub fn effect(self) -> PresetEffect {
        use Modifier::*;
        let key = |m: &[Modifier], k: &str| PresetEffect::Key(Combo::new(m, k));
        match self {
            Preset::CloseTab => key(&[Ctrl], "w"),
            Preset::CloseTabMdi => key(&[Ctrl], "f4"),
            Preset::CloseWindow => key(&[Alt], "f4"),
            Preset::BrowserBack => PresetEffect::AppCommand(AppCommand::BrowserBack),
            Preset::BrowserForward => PresetEffect::AppCommand(AppCommand::BrowserForward),
            Preset::PlayPause => key(&[], "media_playpause"),
            Preset::NextTrack => key(&[], "media_next"),
            Preset::PrevTrack => key(&[], "media_prev"),
            Preset::Mute => key(&[], "volume_mute"),
            Preset::VolumeUp => key(&[], "volume_up"),
            Preset::VolumeDown => key(&[], "volume_down"),
            Preset::DesktopLeft => key(&[Win, Ctrl], "left"),
            Preset::DesktopRight => key(&[Win, Ctrl], "right"),
            Preset::TaskView => key(&[Win], "tab"),
            Preset::Copy => key(&[Ctrl], "c"),
            Preset::Paste => key(&[Ctrl], "v"),
            Preset::Undo => key(&[Ctrl], "z"),
            Preset::Redo => key(&[Ctrl], "y"),
        }
    }
}

impl FromStr for Preset {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim().to_ascii_lowercase();
        Preset::ALL
            .into_iter()
            .find(|p| p.name() == s)
            .ok_or_else(|| format!("unknown preset '{s}'"))
    }
}

/// Change the DPI of the mouse that was clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpiCmd {
    /// Next value in the mouse's DPI list (wraps around).
    Next,
    /// Previous value (wraps around).
    Prev,
    /// A specific value.
    Set(u32),
}

impl FromStr for DpiCmd {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim() {
            "next" => Ok(DpiCmd::Next),
            "prev" => Ok(DpiCmd::Prev),
            n => {
                let v: u32 = n.parse().map_err(|_| format!("unknown DPI command '{n}' (use next, prev or a number)"))?;
                if (100..=30000).contains(&v) { Ok(DpiCmd::Set(v)) } else { Err(format!("DPI {v} is out of range (100-30000)")) }
            }
        }
    }
}

impl fmt::Display for DpiCmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DpiCmd::Next => f.write_str("next"),
            DpiCmd::Prev => f.write_str("prev"),
            DpiCmd::Set(v) => write!(f, "{v}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Leave the button unchanged.
    Passthrough,
    /// Swallow the click.
    None,
    Preset(Preset),
    Key(Combo),
    Mouse(Button),
    Run(String),
    /// Change the clicked mouse's DPI.
    Dpi(DpiCmd),
}

impl FromStr for Action {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim();
        match s {
            "passthrough" => return Ok(Action::Passthrough),
            "none" => return Ok(Action::None),
            _ => {}
        }
        let (kind, rest) = s.split_once(':').ok_or_else(|| format!("invalid action '{s}'"))?;
        match kind {
            "preset" => Ok(Action::Preset(rest.parse()?)),
            "key" => Ok(Action::Key(rest.parse()?)),
            "mouse" => Ok(Action::Mouse(rest.parse()?)),
            "dpi" => Ok(Action::Dpi(rest.parse()?)),
            "run" if !rest.trim().is_empty() => Ok(Action::Run(rest.trim().to_string())),
            "run" => Err("run: needs a program path".into()),
            _ => Err(format!("unknown action type '{kind}'")),
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Passthrough => f.write_str("passthrough"),
            Action::None => f.write_str("none"),
            Action::Preset(p) => write!(f, "preset:{}", p.name()),
            Action::Key(c) => write!(f, "key:{c}"),
            Action::Mouse(b) => write!(f, "mouse:{b}"),
            Action::Run(p) => write!(f, "run:{p}"),
            Action::Dpi(c) => write!(f, "dpi:{c}"),
        }
    }
}

impl Serialize for Action {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combo_roundtrip() {
        let c: Combo = "Ctrl+Shift+W".parse().unwrap();
        assert_eq!(c.to_string(), "ctrl+shift+w");
        assert_eq!(c, "shift+ctrl+w".parse().unwrap());
    }

    #[test]
    fn combo_errors() {
        assert!("ctrl+".parse::<Combo>().is_err());
        assert!("hyper+w".parse::<Combo>().is_err());
        assert!("ctrl+nokey".parse::<Combo>().is_err());
    }

    #[test]
    fn action_roundtrip() {
        for s in ["passthrough", "none", "preset:close_tab", "key:ctrl+w", "mouse:middle", "dpi:next", "dpi:prev", "dpi:1600", "run:C:\\a b\\x.exe"] {
            let a: Action = s.parse().unwrap();
            assert_eq!(a.to_string(), s);
        }
    }

    #[test]
    fn action_errors() {
        assert!("bogus".parse::<Action>().is_err());
        assert!("preset:nope".parse::<Action>().is_err());
        assert!("run:".parse::<Action>().is_err());
        assert!("dpi:fast".parse::<Action>().is_err());
        assert!("dpi:50".parse::<Action>().is_err());
    }

    #[test]
    fn all_presets_roundtrip() {
        for p in Preset::ALL {
            assert_eq!(p.name().parse::<Preset>().unwrap(), p);
            if let PresetEffect::Key(c) = p.effect() {
                assert!(c.to_string().parse::<Combo>().is_ok());
            }
        }
    }
}

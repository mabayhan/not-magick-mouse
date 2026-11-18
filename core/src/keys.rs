//! Neutral key names. Combos store names, never OS key codes, so other
//! platforms can add their own name -> code table.

pub fn is_valid_key(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n.len() == 1 {
        let c = n.as_bytes()[0];
        return c.is_ascii_lowercase() || c.is_ascii_digit();
    }
    if let Some(num) = n.strip_prefix('f') {
        if let Ok(v) = num.parse::<u8>() {
            return (1..=24).contains(&v);
        }
    }
    NAMED_KEYS.contains(&n.as_str())
}

pub const NAMED_KEYS: &[&str] = &[
    "enter", "esc", "tab", "space", "backspace", "delete", "insert", "home", "end", "pageup",
    "pagedown", "up", "down", "left", "right", "printscreen", "pause", "capslock", "menu",
    "minus", "equals", "comma", "period", "slash", "backslash", "semicolon", "quote",
    "backtick", "lbracket", "rbracket",
    "media_playpause", "media_next", "media_prev", "media_stop",
    "volume_up", "volume_down", "volume_mute",
    "browser_back", "browser_forward", "browser_refresh", "browser_home",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Win,
}

impl Modifier {
    pub const ALL: [Modifier; 4] = [Modifier::Ctrl, Modifier::Shift, Modifier::Alt, Modifier::Win];

    pub fn name(self) -> &'static str {
        match self {
            Modifier::Ctrl => "ctrl",
            Modifier::Shift => "shift",
            Modifier::Alt => "alt",
            Modifier::Win => "win",
        }
    }

    pub fn parse(s: &str) -> Option<Modifier> {
        match s.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(Modifier::Ctrl),
            "shift" => Some(Modifier::Shift),
            "alt" => Some(Modifier::Alt),
            "win" | "super" | "meta" => Some(Modifier::Win),
            _ => None,
        }
    }
}

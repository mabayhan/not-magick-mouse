//! Neutral key name -> Linux input event code (linux/input-event-codes.h).

use nmm_core::keys::Modifier;

pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_LEFTMETA: u16 = 125;

pub fn modifier_code(m: Modifier) -> u16 {
    match m {
        Modifier::Ctrl => KEY_LEFTCTRL,
        Modifier::Shift => KEY_LEFTSHIFT,
        Modifier::Alt => KEY_LEFTALT,
        Modifier::Win => KEY_LEFTMETA,
    }
}

const LETTERS: [u16; 26] = [
    30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, // a-m
    49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45, 21, 44, // n-z
];

pub fn key_code(name: &str) -> Option<u16> {
    let n = name.to_ascii_lowercase();
    let b = n.as_bytes();
    if b.len() == 1 {
        let c = b[0];
        if c.is_ascii_lowercase() {
            return Some(LETTERS[(c - b'a') as usize]);
        }
        if c.is_ascii_digit() {
            return Some(if c == b'0' { 11 } else { 2 + (c - b'1') as u16 });
        }
    }
    if let Some(num) = n.strip_prefix('f') {
        if let Ok(v) = num.parse::<u16>() {
            return match v {
                1..=10 => Some(58 + v),
                11 | 12 => Some(76 + v),
                13..=24 => Some(170 + v),
                _ => None,
            };
        }
    }
    Some(match n.as_str() {
        "enter" => 28,
        "esc" => 1,
        "tab" => 15,
        "space" => 57,
        "backspace" => 14,
        "delete" => 111,
        "insert" => 110,
        "home" => 102,
        "end" => 107,
        "pageup" => 104,
        "pagedown" => 109,
        "up" => 103,
        "down" => 108,
        "left" => 105,
        "right" => 106,
        "printscreen" => 99,
        "pause" => 119,
        "capslock" => 58,
        "menu" => 127,
        "minus" => 12,
        "equals" => 13,
        "comma" => 51,
        "period" => 52,
        "slash" => 53,
        "backslash" => 43,
        "semicolon" => 39,
        "quote" => 40,
        "backtick" => 41,
        "lbracket" => 26,
        "rbracket" => 27,
        "media_playpause" => 164,
        "media_next" => 163,
        "media_prev" => 165,
        "media_stop" => 166,
        "volume_up" => 115,
        "volume_down" => 114,
        "volume_mute" => 113,
        "browser_back" => 158,
        "browser_forward" => 159,
        "browser_refresh" => 173,
        "browser_home" => 172,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nmm_core::keys::NAMED_KEYS;

    #[test]
    fn every_named_key_maps() {
        for k in NAMED_KEYS {
            assert!(key_code(k).is_some(), "{k}");
        }
        for c in ('a'..='z').chain('0'..='9') {
            assert!(key_code(&c.to_string()).is_some());
        }
        for f in 1..=24 {
            assert!(key_code(&format!("f{f}")).is_some());
        }
    }

    #[test]
    fn spot_checks() {
        assert_eq!(key_code("w"), Some(17));
        assert_eq!(key_code("a"), Some(30));
        assert_eq!(key_code("z"), Some(44));
        assert_eq!(key_code("0"), Some(11));
        assert_eq!(key_code("1"), Some(2));
        assert_eq!(key_code("f4"), Some(62));
        assert_eq!(key_code("f11"), Some(87));
        assert_eq!(key_code("f12"), Some(88));
        assert_eq!(key_code("f13"), Some(183));
        assert_eq!(key_code("f24"), Some(194));
    }
}

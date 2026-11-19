//! Neutral key name -> Windows virtual-key code.

use nmm_core::keys::Modifier;

pub const VK_CTRL: u16 = 0xA2;
pub const VK_SHIFT: u16 = 0xA0;
pub const VK_ALT: u16 = 0xA4;
pub const VK_WIN: u16 = 0x5B;

pub fn modifier_vk(m: Modifier) -> u16 {
    match m {
        Modifier::Ctrl => VK_CTRL,
        Modifier::Shift => VK_SHIFT,
        Modifier::Alt => VK_ALT,
        Modifier::Win => VK_WIN,
    }
}

/// Returns `(virtual key, needs the extended-key flag)`.
pub fn key_vk(name: &str) -> Option<(u16, bool)> {
    let n = name.to_ascii_lowercase();
    let b = n.as_bytes();
    if b.len() == 1 {
        let c = b[0];
        if c.is_ascii_lowercase() {
            return Some((c.to_ascii_uppercase() as u16, false));
        }
        if c.is_ascii_digit() {
            return Some((c as u16, false));
        }
    }
    if let Some(num) = n.strip_prefix('f') {
        if let Ok(v) = num.parse::<u16>() {
            if (1..=24).contains(&v) {
                return Some((0x70 + v - 1, false));
            }
        }
    }
    Some(match n.as_str() {
        "enter" => (0x0D, false),
        "esc" => (0x1B, false),
        "tab" => (0x09, false),
        "space" => (0x20, false),
        "backspace" => (0x08, false),
        "delete" => (0x2E, true),
        "insert" => (0x2D, true),
        "home" => (0x24, true),
        "end" => (0x23, true),
        "pageup" => (0x21, true),
        "pagedown" => (0x22, true),
        "up" => (0x26, true),
        "down" => (0x28, true),
        "left" => (0x25, true),
        "right" => (0x27, true),
        "printscreen" => (0x2C, false),
        "pause" => (0x13, false),
        "capslock" => (0x14, false),
        "menu" => (0x5D, true),
        "minus" => (0xBD, false),
        "equals" => (0xBB, false),
        "comma" => (0xBC, false),
        "period" => (0xBE, false),
        "slash" => (0xBF, false),
        "backslash" => (0xDC, false),
        "semicolon" => (0xBA, false),
        "quote" => (0xDE, false),
        "backtick" => (0xC0, false),
        "lbracket" => (0xDB, false),
        "rbracket" => (0xDD, false),
        "media_playpause" => (0xB3, false),
        "media_next" => (0xB0, false),
        "media_prev" => (0xB1, false),
        "media_stop" => (0xB2, false),
        "volume_up" => (0xAF, false),
        "volume_down" => (0xAE, false),
        "volume_mute" => (0xAD, false),
        "browser_back" => (0xA6, false),
        "browser_forward" => (0xA7, false),
        "browser_refresh" => (0xA8, false),
        "browser_home" => (0xAC, false),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_valid_core_key_has_a_vk() {
        for k in nmm_core::keys::NAMED_KEYS {
            assert!(key_vk(k).is_some(), "{k}");
        }
        for k in ["a", "z", "0", "9", "f1", "f24"] {
            assert!(key_vk(k).is_some(), "{k}");
        }
        assert!(key_vk("f25").is_none());
    }
}

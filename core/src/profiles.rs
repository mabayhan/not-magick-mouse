use crate::action::Action;
use crate::button::Button;
use crate::config::Config;

/// Resolve what a button press should do for the focused app (`exe` is the
/// bare file name, e.g. `chrome.exe`; `None` when unknown).
///
/// `device` is the hardware key of the mouse that produced the click
/// (`vid_xxxx&pid_xxxx`), `None` when it could not be identified.
///
/// Precedence, most specific first: this mouse inside this app, the app
/// (any mouse), this mouse (any app), global. Each layer overrides only the
/// buttons it defines. A `disabled` mouse, app, or mouse-in-app makes
/// everything passthrough.
pub fn resolve(cfg: &Config, exe: Option<&str>, device: Option<&str>, button: Button) -> Action {
    let dev = device.and_then(|d| cfg.device(d));
    if dev.is_some_and(|d| d.disabled) {
        return Action::Passthrough;
    }
    if let Some(exe) = exe {
        if let Some(app) = cfg.apps.iter().find(|a| a.exe.eq_ignore_ascii_case(exe)) {
            if app.disabled {
                return Action::Passthrough;
            }
            if let Some(ad) = device.and_then(|d| app.device(d)) {
                if ad.disabled {
                    return Action::Passthrough;
                }
                if let Some(a) = ad.buttons.get(button) {
                    return a.clone();
                }
            }
            if let Some(a) = app.buttons.get(button) {
                return a.clone();
            }
        }
    }
    if let Some(a) = dev.and_then(|d| d.buttons.get(button)) {
        return a.clone();
    }
    cfg.global.get(button).cloned().unwrap_or(Action::Passthrough)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config::parse(
            r#"
[global]
x1 = "key:ctrl+w"
x2 = "preset:browser_forward"
[[app]]
exe = "WindowsTerminal.exe"
x1 = "key:ctrl+shift+w"
[[app]]
exe = "game.exe"
disabled = true
"#,
        )
        .unwrap()
    }

    #[test]
    fn empty_is_passthrough() {
        let c = Config::default();
        for b in Button::ALL {
            assert_eq!(resolve(&c, Some("any.exe"), None, b), Action::Passthrough);
        }
    }

    #[test]
    fn global_applies() {
        assert_eq!(resolve(&cfg(), Some("chrome.exe"), None, Button::X1).to_string(), "key:ctrl+w");
        assert_eq!(resolve(&cfg(), None, None, Button::X1).to_string(), "key:ctrl+w");
    }

    #[test]
    fn app_overrides_and_falls_through() {
        let c = cfg();
        assert_eq!(resolve(&c, Some("windowsterminal.exe"), None, Button::X1).to_string(), "key:ctrl+shift+w");
        assert_eq!(resolve(&c, Some("WindowsTerminal.exe"), None, Button::X2).to_string(), "preset:browser_forward");
    }

    #[test]
    fn device_layer() {
        let c = Config::parse(
            r#"
[global]
x1 = "key:ctrl+w"
[[device]]
id = "vid_1532&pid_005c"
x1 = "key:ctrl+t"
[[device]]
id = "vid_045e&pid_07a5"
disabled = true
[[app]]
exe = "term.exe"
x2 = "key:ctrl+shift+w"
"#,
        )
        .unwrap();
        let r = |exe, dev, b| resolve(&c, exe, dev, b).to_string();
        assert_eq!(r(Some("a.exe"), Some("vid_1532&pid_005c"), Button::X1), "key:ctrl+t");
        assert_eq!(r(Some("a.exe"), Some("VID_1532&PID_005C"), Button::X1), "key:ctrl+t");
        assert_eq!(r(Some("a.exe"), Some("vid_046d&pid_b019"), Button::X1), "key:ctrl+w"); // unknown mouse -> global
        assert_eq!(r(Some("a.exe"), None, Button::X1), "key:ctrl+w");
        assert_eq!(r(Some("a.exe"), Some("vid_045e&pid_07a5"), Button::X1), "passthrough");
        assert_eq!(r(Some("term.exe"), Some("vid_1532&pid_005c"), Button::X2), "key:ctrl+shift+w");
    }

    #[test]
    fn mouse_in_app_layer() {
        let c = Config::parse(
            r#"
[global]
x2 = "key:ctrl+g"
[[device]]
id = "vid_046d&pid_b019"
x2 = "key:ctrl+d"
x1 = "key:ctrl+1"
[[app]]
exe = "chrome.exe"
x1 = "key:ctrl+a"
[[app.device]]
id = "vid_046d&pid_b019"
x2 = "key:ctrl+w"
[[app]]
exe = "vivaldi.exe"
[[app.device]]
id = "vid_046d&pid_b019"
x2 = "key:ctrl+t"
[[app.device]]
id = "vid_1532&pid_005c"
disabled = true
"#,
        )
        .unwrap();
        let mx = Some("vid_046d&pid_b019");
        let rz = Some("vid_1532&pid_005c");
        let r = |exe, dev, b| resolve(&c, Some(exe), dev, b).to_string();
        assert_eq!(r("chrome.exe", mx, Button::X2), "key:ctrl+w"); // app + mouse
        assert_eq!(r("vivaldi.exe", mx, Button::X2), "key:ctrl+t");
        assert_eq!(r("chrome.exe", mx, Button::X1), "key:ctrl+a"); // app beats mouse
        assert_eq!(r("chrome.exe", rz, Button::X2), "key:ctrl+g"); // other mouse: global
        assert_eq!(r("vivaldi.exe", mx, Button::X1), "key:ctrl+1"); // falls to mouse profile
        assert_eq!(r("vivaldi.exe", rz, Button::X2), "passthrough"); // disabled combo
        assert_eq!(r("other.exe", mx, Button::X2), "key:ctrl+d"); // mouse, any app
        let again = Config::parse(&c.to_toml().unwrap()).unwrap();
        assert_eq!(again, c);
    }

    #[test]
    fn disabled_app() {
        assert_eq!(resolve(&cfg(), Some("game.exe"), None, Button::X1), Action::Passthrough);
    }
}

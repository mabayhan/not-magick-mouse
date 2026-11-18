use crate::action::Action;
use crate::button::Button;
use crate::config::Config;

/// Resolve what a button press should do for the focused app (`exe` is the
/// bare file name, e.g. `chrome.exe`; `None` when unknown).
///
/// An app profile overrides only the buttons it defines; the rest fall
/// through to global. A `disabled` profile makes everything passthrough.
pub fn resolve(cfg: &Config, exe: Option<&str>, button: Button) -> Action {
    if let Some(exe) = exe {
        if let Some(app) = cfg.apps.iter().find(|a| a.exe.eq_ignore_ascii_case(exe)) {
            if app.disabled {
                return Action::Passthrough;
            }
            if let Some(a) = app.buttons.get(button) {
                return a.clone();
            }
        }
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
            assert_eq!(resolve(&c, Some("any.exe"), b), Action::Passthrough);
        }
    }

    #[test]
    fn global_applies() {
        assert_eq!(resolve(&cfg(), Some("chrome.exe"), Button::X1).to_string(), "key:ctrl+w");
        assert_eq!(resolve(&cfg(), None, Button::X1).to_string(), "key:ctrl+w");
    }

    #[test]
    fn app_overrides_and_falls_through() {
        let c = cfg();
        assert_eq!(resolve(&c, Some("windowsterminal.exe"), Button::X1).to_string(), "key:ctrl+shift+w");
        assert_eq!(resolve(&c, Some("WindowsTerminal.exe"), Button::X2).to_string(), "preset:browser_forward");
    }

    #[test]
    fn disabled_app() {
        assert_eq!(resolve(&cfg(), Some("game.exe"), Button::X1), Action::Passthrough);
    }
}

//! Synthesised input: a virtual keyboard for shortcuts and `run:` actions.

use crate::keymap::{key_code, modifier_code};
use anyhow::{Context, Result};
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};
use nmm_core::action::{Action, AppCommand, Combo, PresetEffect};
use nmm_core::keys::Modifier;
use std::sync::Mutex;

pub const VIRTUAL_PREFIX: &str = "nmm virtual";

pub struct Keyboard {
    dev: Mutex<VirtualDevice>,
}

impl Keyboard {
    pub fn new() -> Result<Keyboard> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for c in 1..=255u16 {
            keys.insert(KeyCode::new(c));
        }
        let dev = VirtualDevice::builder()
            .context("cannot open /dev/uinput: add yourself to the input group (sudo usermod -aG input $USER), then log out and back in")?
            .name(&format!("{VIRTUAL_PREFIX} keyboard"))
            .with_keys(&keys)?
            .build()?;
        Ok(Keyboard { dev: Mutex::new(dev) })
    }

    fn emit(&self, events: &[InputEvent]) {
        if let Ok(mut d) = self.dev.lock() {
            let _ = d.emit(events);
        }
    }

    fn key(code: u16, down: bool) -> InputEvent {
        InputEvent::new(EventType::KEY.0, code, down as i32)
    }

    pub fn send_combo(&self, c: &Combo) {
        let Some(code) = key_code(&c.key) else { return };
        let mods: Vec<u16> = c.mods.iter().map(|m: &Modifier| modifier_code(*m)).collect();
        // One SYN-terminated batch per step so apps see modifiers before the key.
        for m in &mods {
            self.emit(&[Self::key(*m, true)]);
        }
        self.emit(&[Self::key(code, true)]);
        self.emit(&[Self::key(code, false)]);
        for m in mods.iter().rev() {
            self.emit(&[Self::key(*m, false)]);
        }
    }

    /// Everything except `Mouse`/`Dpi`, which the device loop handles.
    pub fn perform(&self, a: &Action) {
        match a {
            Action::Passthrough | Action::None | Action::Mouse(_) | Action::Dpi(_) => {}
            Action::Key(c) => self.send_combo(c),
            Action::Preset(p) => match p.effect() {
                PresetEffect::Key(c) => self.send_combo(&c),
                PresetEffect::AppCommand(cmd) => {
                    let name = match cmd {
                        AppCommand::BrowserBack => "left",
                        AppCommand::BrowserForward => "right",
                    };
                    self.send_combo(&Combo::new(&[Modifier::Alt], name));
                }
            },
            Action::Run(cmd) => run_program(cmd),
        }
    }
}

fn run_program(cmd: &str) {
    let cmd = cmd.to_string();
    std::thread::spawn(move || {
        let _ = std::process::Command::new("sh").arg("-c").arg(&cmd).spawn().and_then(|mut c| c.wait());
    });
}

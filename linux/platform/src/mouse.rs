//! One grabbed physical mouse: events are read exclusively, buttons are
//! remapped, everything else is forwarded unchanged through a uinput clone.

use crate::output::{Keyboard, VIRTUAL_PREFIX};
use anyhow::{Context, Result};
use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, Device, EventType, InputEvent, KeyCode, RelativeAxisCode, SynchronizationCode};
use nmm_core::config::Config;
use nmm_core::profiles::resolve;
use nmm_core::{Action, Button};
use std::path::Path;
use std::sync::{Mutex, RwLock};

/// State shared by every device thread.
pub struct Shared {
    pub cfg: RwLock<Config>,
    pub exe: std::sync::Arc<Mutex<Option<String>>>,
    pub kbd: Keyboard,
}

const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const BTN_SIDE: u16 = 0x113;
const BTN_EXTRA: u16 = 0x114;
const BTN_FORWARD: u16 = 0x115;
const BTN_BACK: u16 = 0x116;

fn button_of(code: u16) -> Option<Button> {
    Some(match code {
        BTN_LEFT => Button::Left,
        BTN_RIGHT => Button::Right,
        BTN_MIDDLE => Button::Middle,
        BTN_SIDE | BTN_BACK => Button::X1,
        BTN_EXTRA | BTN_FORWARD => Button::X2,
        _ => return None,
    })
}

fn code_of(b: Button) -> u16 {
    match b {
        Button::Left => BTN_LEFT,
        Button::Right => BTN_RIGHT,
        Button::Middle => BTN_MIDDLE,
        Button::X1 => BTN_SIDE,
        Button::X2 => BTN_EXTRA,
    }
}

/// A pointer: has a left button and relative motion (touchpads and tablets report absolute axes).
pub fn is_mouse(dev: &Device) -> bool {
    let has_btn = dev.supported_keys().is_some_and(|k| k.contains(KeyCode::new(BTN_LEFT)));
    let has_rel = dev.supported_relative_axes().is_some_and(|r| r.contains(RelativeAxisCode::REL_X));
    has_btn && has_rel
}

pub fn is_ours(dev: &Device) -> bool {
    dev.name().is_some_and(|n| n.starts_with(VIRTUAL_PREFIX))
}

/// `vid_xxxx&pid_xxxx`, the same key `[[device]]` profiles use on Windows.
pub fn device_key(dev: &Device) -> String {
    let id = dev.input_id();
    format!("vid_{:04x}&pid_{:04x}", id.vendor(), id.product())
}

#[derive(Clone, Copy, PartialEq)]
enum Held {
    No,
    Swallow,
    Mouse(Button),
}

fn key_event(code: u16, value: i32) -> InputEvent {
    InputEvent::new(EventType::KEY.0, code, value)
}

/// Blocks until the device disappears or errors.
pub fn run_device(path: &Path, shared: &Shared) -> Result<()> {
    let mut dev = Device::open(path).with_context(|| format!("open {}", path.display()))?;
    let key = device_key(&dev);
    let name = dev.name().unwrap_or("mouse").to_string();

    let mut keys = AttributeSet::<KeyCode>::new();
    for c in dev.supported_keys().into_iter().flat_map(|k| k.iter()) {
        keys.insert(c);
    }
    for b in Button::ALL {
        keys.insert(KeyCode::new(code_of(b))); // any button can be a remap target
    }
    let vname = format!("{VIRTUAL_PREFIX} {name}");
    let mut builder = VirtualDevice::builder()?.name(&vname).input_id(dev.input_id()).with_keys(&keys)?;
    if let Some(rel) = dev.supported_relative_axes() {
        builder = builder.with_relative_axes(rel)?;
    }
    let mut out = builder.build()?;
    dev.grab().with_context(|| format!("grab {name} (another program may hold it)"))?;
    eprintln!("nmmd: remapping {name} ({key}) at {}", path.display());

    let mut held = [Held::No; 5];
    let mut batch: Vec<InputEvent> = Vec::new();
    let mut deferred: Vec<Action> = Vec::new();

    loop {
        for ev in dev.fetch_events()? {
            let et = ev.event_type();
            if et == EventType::SYNCHRONIZATION {
                if ev.code() == SynchronizationCode::SYN_REPORT.0 {
                    if !batch.is_empty() {
                        out.emit(&batch)?; // emit appends the SYN_REPORT
                        batch.clear();
                    }
                    for a in deferred.drain(..) {
                        shared.kbd.perform(&a);
                    }
                }
                continue; // SYN_DROPPED etc.: nothing to forward
            }
            if et == EventType::KEY {
                if let Some(b) = button_of(ev.code()) {
                    handle_button(shared, &key, b, ev, &mut held, &mut batch, &mut deferred);
                    continue;
                }
                batch.push(ev); // other buttons / keys
            } else if et == EventType::RELATIVE {
                batch.push(ev);
            }
            // EV_MSC (scancodes) and the rest are dropped: the clone does not declare them.
        }
    }
}

fn handle_button(shared: &Shared, key: &str, b: Button, ev: InputEvent, held: &mut [Held; 5], batch: &mut Vec<InputEvent>, deferred: &mut Vec<Action>) {
    let i = b as usize;
    match ev.value() {
        1 => {
            let exe = shared.exe.lock().ok().and_then(|g| g.clone());
            let action = match shared.cfg.read() {
                Ok(cfg) => resolve(&cfg, exe.as_deref(), Some(key), b),
                Err(_) => Action::Passthrough,
            };
            held[i] = match &action {
                Action::Passthrough => Held::No,
                Action::Mouse(t) => Held::Mouse(*t),
                _ => Held::Swallow,
            };
            match action {
                Action::Passthrough => batch.push(ev),
                Action::None => {}
                Action::Mouse(t) => batch.push(key_event(code_of(t), 1)),
                Action::Dpi(_) => eprintln!("nmmd: DPI actions are not supported on Linux yet"),
                other => deferred.push(other),
            }
        }
        0 => match std::mem::replace(&mut held[i], Held::No) {
            Held::No => batch.push(ev),
            Held::Swallow => {}
            Held::Mouse(t) => batch.push(key_event(code_of(t), 0)),
        },
        _ => {
            // Auto-repeat: forward only for passthrough buttons.
            if held[i] == Held::No {
                batch.push(ev);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_codes_round_trip() {
        for b in Button::ALL {
            assert_eq!(button_of(code_of(b)), Some(b));
        }
        assert_eq!(button_of(BTN_BACK), Some(Button::X1));
        assert_eq!(button_of(BTN_FORWARD), Some(Button::X2));
        assert_eq!(button_of(30), None);
    }
}

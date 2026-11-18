//! Synthesises keyboard / mouse input. Every event carries `TAG` in
//! `dwExtraInfo` so the mouse hook can ignore its own output.

use crate::keymap::{key_vk, modifier_vk};
use nmm_core::action::{Action, AppCommand, Combo, PresetEffect};
use nmm_core::Button;
use std::os::windows::process::CommandExt;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub const TAG: usize = 0x4E4D_4D21;

fn key_input(vk: u16, extended: bool, up: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: TAG },
        },
    }
}

fn send(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    unsafe {
        SendInput(inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn send_combo(c: &Combo) {
    let Some((vk, ext)) = key_vk(&c.key) else { return };
    let mut v = Vec::with_capacity(c.mods.len() * 2 + 2);
    for m in &c.mods {
        v.push(key_input(modifier_vk(*m), false, false));
    }
    v.push(key_input(vk, ext, false));
    v.push(key_input(vk, ext, true));
    for m in c.mods.iter().rev() {
        v.push(key_input(modifier_vk(*m), false, true));
    }
    send(&v);
}

pub fn mouse_button(b: Button, down: bool) {
    let (flags, data) = match (b, down) {
        (Button::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
        (Button::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
        (Button::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
        (Button::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
        (Button::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
        (Button::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
        (Button::X1, true) => (MOUSEEVENTF_XDOWN, 1),
        (Button::X1, false) => (MOUSEEVENTF_XUP, 1),
        (Button::X2, true) => (MOUSEEVENTF_XDOWN, 2),
        (Button::X2, false) => (MOUSEEVENTF_XUP, 2),
    };
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: data, dwFlags: flags, time: 0, dwExtraInfo: TAG },
        },
    };
    send(&[input]);
}

fn app_command(cmd: AppCommand) {
    let vk = match cmd {
        AppCommand::BrowserBack => 0xA6,
        AppCommand::BrowserForward => 0xA7,
    };
    send(&[key_input(vk, false, false), key_input(vk, false, true)]);
}

fn run_program(cmdline: &str) {
    let cmdline = cmdline.to_string();
    // Off the hook thread: process creation can be slow.
    std::thread::spawn(move || {
        let mut c = std::process::Command::new("cmd");
        c.raw_arg("/C start \"\" ").raw_arg(&cmdline);
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let _ = c.spawn().and_then(|mut ch| ch.wait());
    });
}

/// Perform a non-button-mirroring action. (`Mouse` is handled by the hook so
/// that press and release map to press and release.)
pub fn perform(a: &Action) {
    match a {
        Action::Passthrough | Action::None | Action::Mouse(_) => {}
        Action::Key(c) => send_combo(c),
        Action::Preset(p) => match p.effect() {
            PresetEffect::Key(c) => send_combo(&c),
            PresetEffect::AppCommand(c) => app_command(c),
        },
        Action::Run(cmd) => run_program(cmd),
    }
}

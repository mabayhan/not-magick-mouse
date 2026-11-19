//! Focused application via X11 (`_NET_ACTIVE_WINDOW` + `_NET_WM_PID`).
//! Wayland sessions have no portable equivalent, so `exe` stays `None` there
//! (XWayland windows are still seen).

use std::sync::{Arc, Mutex};
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

pub fn spawn(exe: Arc<Mutex<Option<String>>>) {
    std::thread::spawn(move || {
        loop {
            // (Re)connect: the X server can start after the daemon (login) or restart.
            if let Ok((conn, screen)) = x11rb::connect(None) {
                let root = conn.setup().roots[screen].root;
                let atom = |name: &[u8]| conn.intern_atom(false, name).ok().and_then(|c| c.reply().ok()).map(|r| r.atom);
                if let (Some(active), Some(pid_atom)) = (atom(b"_NET_ACTIVE_WINDOW"), atom(b"_NET_WM_PID")) {
                    // An error means the connection was lost: fall out and reconnect.
                    while let Ok(n) = active_exe(&conn, root, active, pid_atom) {
                        if let Ok(mut g) = exe.lock() {
                            *g = n;
                        }
                        std::thread::sleep(Duration::from_millis(250));
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

fn active_exe(conn: &impl Connection, root: u32, active: u32, pid_atom: u32) -> Result<Option<String>, x11rb::errors::ReplyError> {
    let r = conn.get_property(false, root, active, AtomEnum::WINDOW, 0, 1)?.reply()?;
    let Some(win) = r.value32().and_then(|mut v| v.next()).filter(|w| *w != 0) else { return Ok(None) };
    let r = conn.get_property(false, win, pid_atom, AtomEnum::CARDINAL, 0, 1)?.reply()?;
    let Some(pid) = r.value32().and_then(|mut v| v.next()) else { return Ok(None) };
    Ok(exe_name(pid))
}

fn exe_name(pid: u32) -> Option<String> {
    let link = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
    // Strip the " (deleted)" suffix left after an upgrade replaces the binary.
    let name = link.and_then(|p| p.file_name().map(|n| n.to_string_lossy().trim_end_matches(" (deleted)").to_string()));
    name.or_else(|| std::fs::read_to_string(format!("/proc/{pid}/comm")).ok().map(|s| s.trim().to_string()))
}

//! Small pieces shared by the daemon and the GUI: the "pause remapping while a
//! button is being detected" file, a per-user single-instance lock and the
//! runtime directory both use.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// How long a pause request stays valid. The GUI renews it every two seconds.
const SUSPEND_TIMEOUT: Duration = Duration::from_secs(6);

pub(crate) fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

fn suspend_file() -> PathBuf {
    runtime_dir().join("nmm-suspend")
}

/// Pause (true) or resume (false) remapping in the running daemon, e.g. while
/// the GUI detects a physical button. Expires by itself unless renewed.
pub fn set_remap_suspended(suspend: bool) {
    let f = suspend_file();
    if suspend {
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let _ = std::fs::write(&f, now.to_string()); // rewriting bumps the mtime
    } else {
        let _ = std::fs::remove_file(&f);
    }
}

/// Has the GUI asked to pause remapping recently?
pub(crate) fn remap_suspended() -> bool {
    std::fs::metadata(suspend_file())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < SUSPEND_TIMEOUT)
}

/// The daemon watches the config file's modification time, so nothing needs to be sent.
pub fn notify_reload() {}

/// Takes a lock named `name` for as long as the returned file lives. `None` if another process holds it.
pub(crate) fn lock_instance(name: &str) -> Option<std::fs::File> {
    let f = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(runtime_dir().join(format!("{name}.lock"))).ok()?;
    f.try_lock().ok()?;
    Some(f)
}

/// GUI single-instance guard: the lock is intentionally leaked so it lives as long as the process.
pub fn single_instance(name: &str) -> bool {
    match lock_instance(name) {
        Some(f) => {
            std::mem::forget(f);
            true
        }
        None => false,
    }
}

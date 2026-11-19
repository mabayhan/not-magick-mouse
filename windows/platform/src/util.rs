use windows::Win32::Foundation::*;
use windows::Win32::System::Registry::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PCWSTR, PWSTR};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Registered message the GUI broadcasts after saving the config.
pub fn reload_message() -> u32 {
    let name = wide("NMM_RELOAD");
    unsafe { RegisterWindowMessageW(PCWSTR(name.as_ptr())) }
}

pub fn suspend_message() -> u32 {
    let name = wide("NMM_SUSPEND");
    unsafe { RegisterWindowMessageW(PCWSTR(name.as_ptr())) }
}

/// Pause (true) or resume (false) remapping in a running daemon, e.g. while
/// the GUI detects a physical button. The daemon auto-resumes after a few
/// seconds unless the pause is renewed.
pub fn set_remap_suspended(suspend: bool) {
    unsafe {
        let _ = PostMessageW(Some(HWND_BROADCAST), suspend_message(), WPARAM(suspend as usize), LPARAM(0));
    }
}

/// Ask a running daemon to reload its config.
pub fn notify_reload() {
    unsafe {
        let _ = PostMessageW(Some(HWND_BROADCAST), reload_message(), WPARAM(0), LPARAM(0));
    }
}

/// Returns false if another instance holds the named mutex. The handle is
/// intentionally leaked: it must live as long as the process.
pub fn single_instance(name: &str) -> bool {
    use windows::Win32::System::Threading::CreateMutexW;
    let n = wide(name);
    unsafe {
        match CreateMutexW(None, true, PCWSTR(n.as_ptr())) {
            Ok(_h) => GetLastError() != ERROR_ALREADY_EXISTS,
            Err(_) => false,
        }
    }
}

pub fn error_box(title: &str, text: &str) {
    let (t, x) = (wide(title), wide(text));
    unsafe {
        MessageBoxW(None, PCWSTR(x.as_ptr()), PCWSTR(t.as_ptr()), MB_OK | MB_ICONERROR);
    }
}

/// Bare exe file name (e.g. `chrome.exe`) of the process owning `hwnd`.
pub fn exe_of_window(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().map(str::to_string)
    }
}

/// Distinct exe names of visible, titled top-level windows (for the GUI's app picker).
pub fn list_running_apps() -> Vec<String> {
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        unsafe {
            let out = &mut *(lp.0 as *mut Vec<String>);
            if IsWindowVisible(hwnd).as_bool() && GetWindowTextLengthW(hwnd) > 0 {
                if let Some(e) = exe_of_window(hwnd) {
                    if !out.iter().any(|x| x.eq_ignore_ascii_case(&e)) {
                        out.push(e);
                    }
                }
            }
        }
        BOOL(1)
    }
    let mut out: Vec<String> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut out as *mut _ as isize));
    }
    out.sort_by_key(|s| s.to_ascii_lowercase());
    out
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "not-magick-mouse";

pub fn autostart_enabled() -> bool {
    unsafe {
        let key = wide(RUN_KEY);
        let val = wide(RUN_VALUE);
        let mut h = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), None, KEY_READ, &mut h).is_err() {
            return false;
        }
        let r = RegQueryValueExW(h, PCWSTR(val.as_ptr()), None, None, None, None);
        let _ = RegCloseKey(h);
        r.is_ok()
    }
}

/// Add or remove the HKCU Run entry. `config` is passed as `--config` when
/// the config is not at the default location.
pub fn set_autostart(enable: bool, config: Option<&std::path::Path>) {
    unsafe {
        let key = wide(RUN_KEY);
        let val = wide(RUN_VALUE);
        let mut h = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr()), None, KEY_SET_VALUE, &mut h).is_err() {
            return;
        }
        if enable {
            if let Ok(exe) = std::env::current_exe() {
                let mut line = format!("\"{}\"", exe.display());
                if let Some(c) = config {
                    line.push_str(&format!(" --config \"{}\"", c.display()));
                }
                let cmd = wide(&line);
                let bytes = std::slice::from_raw_parts(cmd.as_ptr() as *const u8, cmd.len() * 2);
                let _ = RegSetValueExW(h, PCWSTR(val.as_ptr()), None, REG_SZ, Some(bytes));
            }
        } else {
            let _ = RegDeleteValueW(h, PCWSTR(val.as_ptr()));
        }
        let _ = RegCloseKey(h);
    }
}

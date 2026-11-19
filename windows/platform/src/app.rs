//! The daemon: message window, tray icon, mouse hook, foreground watcher,
//! DPI poller and OSD, all driven by one Win32 message loop.

use crate::inject::{self, TAG};
use crate::osd::Osd;
use crate::util::*;
use crate::devices::{self, Tracker};
use crate::dpi;
use nmm_core::config::{Config, default_config_path, saved_config_path};
use nmm_core::profiles::resolve;
use nmm_core::{Action, Button};
use std::cell::RefCell;
use std::sync::Arc;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

const WM_TRAY: u32 = WM_APP + 1;
const WM_DPI: u32 = WM_APP + 2;

const ID_TOGGLE: usize = 1;
const ID_SETTINGS: usize = 2;
const ID_RELOAD: usize = 3;
const ID_AUTOSTART: usize = 4;
const ID_QUIT: usize = 5;
const ID_FOLDER: usize = 6;

#[derive(Clone, Copy, PartialEq)]
enum Held {
    No,
    Swallow,
    Mouse(Button),
}

struct State {
    cfg: Config,
    exe: Option<String>,
    enabled: bool,
    held: [Held; 5],
    osd: Osd,
    dpi: Arc<dpi::Shared>,
    reload_msg: u32,
    suspend_msg: u32,
    suspended: bool,
    tracker: Tracker,
}

const SUSPEND_TIMER: usize = 99;
const SUSPEND_TIMEOUT_MS: u32 = 6000;

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn idx(b: Button) -> usize {
    b as usize
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

// ---------- mouse hook ----------

/// Returns true if the event must be swallowed. Runs inside the hook, so it
/// does no I/O besides `SendInput`, and never holds the state borrow while injecting.
fn handle_button(button: Button, down: bool) -> bool {
    if down {
        let (action, device) = with_state(|s| {
            // Which mouse was it? Unknown (None) falls back to global mappings.
            s.tracker.drain();
            let device = s.tracker.take(button, true);
            let a = if s.enabled && !s.suspended { resolve(&s.cfg, s.exe.as_deref(), device.as_deref(), button) } else { Action::Passthrough };
            s.held[idx(button)] = match &a {
                Action::Passthrough => Held::No,
                Action::Mouse(t) => Held::Mouse(*t),
                _ => Held::Swallow,
            };
            (a, device)
        })
        .unwrap_or((Action::Passthrough, None));
        match action {
            Action::Passthrough => false,
            Action::None => true,
            Action::Mouse(t) => {
                inject::mouse_button(t, true);
                true
            }
            Action::Dpi(cmd) => {
                with_state(|s| s.dpi.request(device, cmd));
                true
            }
            other => {
                inject::perform(&other);
                true
            }
        }
    } else {
        let held = with_state(|s| std::mem::replace(&mut s.held[idx(button)], Held::No)).unwrap_or(Held::No);
        match held {
            Held::No => false,
            Held::Swallow => true,
            Held::Mouse(t) => {
                inject::mouse_button(t, false);
                true
            }
        }
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 {
            let info = &*(lp.0 as *const MSLLHOOKSTRUCT);
            if info.dwExtraInfo != TAG {
                let hit = match wp.0 as u32 {
                    WM_LBUTTONDOWN => Some((Button::Left, true)),
                    WM_LBUTTONUP => Some((Button::Left, false)),
                    WM_RBUTTONDOWN => Some((Button::Right, true)),
                    WM_RBUTTONUP => Some((Button::Right, false)),
                    WM_MBUTTONDOWN => Some((Button::Middle, true)),
                    WM_MBUTTONUP => Some((Button::Middle, false)),
                    m @ (WM_XBUTTONDOWN | WM_XBUTTONUP) => match (info.mouseData >> 16) & 0xFFFF {
                        1 => Some((Button::X1, m == WM_XBUTTONDOWN)),
                        2 => Some((Button::X2, m == WM_XBUTTONDOWN)),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some((b, down)) = hit {
                    if handle_button(b, down) {
                        return LRESULT(1);
                    }
                }
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}

// ---------- foreground watcher ----------

unsafe extern "system" fn foreground_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    let exe = exe_of_window(hwnd);
    with_state(|s| s.exe = exe);
}

// ---------- config ----------

/// Path forced by `--config` / `NMM_CONFIG`. When `None`, the path saved from the GUI, then the default, is used (re-read on every reload).
static CONFIG: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();

fn cfg_path() -> std::path::PathBuf {
    CONFIG.get().cloned().flatten().or_else(saved_config_path).unwrap_or_else(default_config_path)
}

/// The forced path, if any: what we pass on to the GUI and to the autostart entry.
fn custom_cfg() -> Option<std::path::PathBuf> {
    CONFIG.get().cloned().flatten()
}

fn apply_config(cfg: &Config) {
    set_autostart_if_changed(cfg.settings.start_with_windows);
    with_state(|s| s.dpi.configure(cfg));
}

fn set_autostart_if_changed(want: bool) {
    if autostart_enabled() != want {
        set_autostart(want, custom_cfg().as_deref());
    }
}

fn show_osd(text: &str) {
    with_state(|s| s.osd.show(text, s.cfg.settings.osd_ms, &s.cfg.settings.osd_position));
}

fn reload() {
    match Config::load(&cfg_path()) {
        Ok(cfg) => {
            apply_config(&cfg);
            with_state(|s| s.cfg = cfg);
        }
        // Keep the last good config so remapping never stops.
        Err(e) => show_osd(&format!("Config error: {}", first_line(&format!("{e:#}")))),
    }
}

fn first_line(s: &str) -> String {
    let l: String = s.lines().next().unwrap_or("").chars().take(60).collect();
    l
}

fn open_settings() {
    if let Ok(me) = std::env::current_exe() {
        let gui = me.with_file_name("nmm-gui.exe");
        let mut c = std::process::Command::new(gui);
        if let Some(p) = custom_cfg() {
            c.arg("--config").arg(p);
        }
        let _ = c.spawn();
    }
}

fn toggle_autostart_in_config() {
    let path = cfg_path();
    if let Ok(mut cfg) = Config::load(&path) {
        cfg.settings.start_with_windows = !autostart_enabled();
        if cfg.save(&path).is_ok() {
            reload();
        }
    }
}

// ---------- tray ----------

fn tray_icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        ..Default::default()
    };
    unsafe {
        // Resource id 1 is the exe's embedded icon; fall back to the stock one.
        let hinst = GetModuleHandleW(None).map(|m| HINSTANCE(m.0)).ok();
        nid.hIcon = LoadIconW(hinst, PCWSTR(1 as *const u16))
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .unwrap_or_default();
    }
    for (i, c) in "not-magick-mouse".encode_utf16().enumerate() {
        nid.szTip[i] = c;
    }
    nid
}

fn show_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let enabled = with_state(|s| s.enabled).unwrap_or(true);
        let item = |id: usize, text: &str, checked: bool| {
            let t = wide(text);
            let flags = MF_STRING | if checked { MF_CHECKED } else { MF_UNCHECKED };
            let _ = AppendMenuW(menu, flags, id, PCWSTR(t.as_ptr()));
        };
        item(ID_TOGGLE, "Enabled", enabled);
        item(ID_SETTINGS, "Settings...", false);
        item(ID_RELOAD, "Reload config", false);
        item(ID_FOLDER, "Open config folder", false);
        item(ID_AUTOSTART, "Start with Windows", autostart_enabled());
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        item(ID_QUIT, "Quit", false);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, None, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if with_state(|s| msg == s.reload_msg).unwrap_or(false) {
            reload();
            return LRESULT(0);
        }
        if with_state(|s| msg == s.suspend_msg).unwrap_or(false) {
            let on = wp.0 != 0;
            with_state(|s| {
                s.suspended = on;
                s.held = [Held::No; 5];
            });
            if on {
                SetTimer(Some(hwnd), SUSPEND_TIMER, SUSPEND_TIMEOUT_MS, None);
            } else {
                let _ = KillTimer(Some(hwnd), SUSPEND_TIMER);
            }
            return LRESULT(0);
        }
        match msg {
            WM_TIMER if wp.0 == SUSPEND_TIMER => {
                let _ = KillTimer(Some(hwnd), SUSPEND_TIMER);
                with_state(|s| s.suspended = false);
                LRESULT(0)
            }
            WM_TRAY => {
                match (lp.0 & 0xFFFF) as u32 {
                    WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                    WM_LBUTTONDBLCLK => open_settings(),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DPI => {
                let events = with_state(|s| s.dpi.take_events()).unwrap_or_default();
                if let Some(e) = events.last() {
                    // With several DPI-capable mice, say which one changed.
                    show_osd(&if let Some(n) = &e.note {
                        n.clone()
                    } else if e.multi { format!("{}: {} DPI", e.name, e.dpi) } else { format!("{} DPI", e.dpi) });
                }
                LRESULT(0)
            }
            WM_INPUT => {
                with_state(|s| s.tracker.on_wm_input(lp));
                DefWindowProcW(hwnd, msg, wp, lp) // required so Windows can clean up the input
            }
            WM_COMMAND => {
                match wp.0 & 0xFFFF {
                    ID_TOGGLE => {
                        with_state(|s| {
                            s.enabled = !s.enabled;
                            s.held = [Held::No; 5];
                        });
                    }
                    ID_SETTINGS => open_settings(),
                    ID_RELOAD => reload(),
                    ID_FOLDER => open_config_folder(),
                    ID_AUTOSTART => toggle_autostart_in_config(),
                    ID_QUIT => {
                        let _ = DestroyWindow(hwnd);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let nid = tray_icon_data(hwnd);
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

/// Run the daemon until Quit. Returns `Ok(false)` if another instance is already running.
pub fn run(explicit_config: Option<std::path::PathBuf>) -> anyhow::Result<bool> {
    let _ = CONFIG.set(explicit_config);
    if !single_instance("Local\\not-magick-mouse-daemon") {
        return Ok(false);
    }

    // A broken config at startup falls back to the empty default; the error is shown on the OSD.
    let (cfg, cfg_err) = match Config::load(&cfg_path()) {
        Ok(c) => (c, None),
        Err(e) => (Config::default(), Some(format!("Config error: {}", first_line(&format!("{e:#}"))))),
    };

    unsafe {
        let hinst = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = wide("NmmDaemon");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        // A real (hidden) top-level window, so it receives the broadcast reload message.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class.as_ptr()),
            PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        )?;

        let poll = Arc::new(dpi::Shared::default());
        let state = State {
            cfg: cfg.clone(),
            exe: exe_of_window(GetForegroundWindow()),
            enabled: true,
            held: [Held::No; 5],
            osd: Osd::new()?,
            dpi: poll.clone(),
            reload_msg: reload_message(),
            suspend_msg: suspend_message(),
            suspended: false,
            tracker: Tracker::default(),
        };
        STATE.with(|s| *s.borrow_mut() = Some(state));
        poll.configure(&cfg);
        apply_config(&cfg);

        let _ = Shell_NotifyIconW(NIM_ADD, &tray_icon_data(hwnd));
        devices::register_raw_mouse(hwnd);
        let _mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(hinst), 0)?;
        let _fg = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        dpi::spawn_worker(hwnd.0 as isize, WM_DPI, poll.clone());
        dpi::spawn(hwnd.0 as isize, WM_DPI, poll);
        if let Some(e) = cfg_err {
            show_osd(&e);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(_mouse);
    }
    Ok(true)
}

fn open_config_folder() {
    let path = cfg_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::process::Command::new("explorer").arg(dir).spawn();
    }
}

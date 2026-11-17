# not-magick-mouse — Plan

A tiny, resource-efficient mouse button remapper with a tray icon. Think "X-Mouse Button Control, minus the bloat" plus the Razer-style DPI on-screen display.

## Goals

1. **Global mappings** — remap mouse buttons to keys / key combos / actions everywhere.
2. **Per-app mappings** — override the global profile when a specific app is focused.
3. **DPI OSD** — show the current DPI value on screen (Razer Synapse style) when it is switched.
4. **System tray** — runs in the tray, no main window; menu for enable/disable, reload config, open config, quit.

Non-goals (v1): macros with timing/recording, RGB/lighting, firmware flashing, cloud sync, a heavy GUI.

## Open decisions (defaults chosen, flag if wrong)

| Decision | Default | Why |
|---|---|---|
| Target OS | **Windows 10/11 first** | X-Mouse Button Control and Razer Synapse are Windows tools; dev happens in WSL, so build cross-compiled / on Windows. Linux (evdev/uinput) is a later, separate backend. |
| Language | **Rust** | Single small static binary, ~no idle CPU, a few MB RAM, no runtime. |
| Config | **TOML file**, hot-reloaded | Simple, hand-editable, diffable. GUI editor is a later phase. |
| DPI detection | **Needs verification** (see Risk #1) | Depends on how the mouse reports DPI changes. |

## Architecture

```
┌──────────────────────────────────────────────┐
│ main (event loop, tray, config watcher)      │
├───────────────┬───────────────┬──────────────┤
│ input hook    │ profile engine│ dpi monitor  │
│ (low-level    │ (global +     │ (HID / hook) │
│  mouse hook)  │  per-app      │      │       │
│      │        │  resolution)  │      ▼       │
│      ▼        │      ▲        │  OSD window  │
│ action        │ foreground    │ (layered,    │
│ injector      │ window watcher│  click-thru) │
│ (SendInput)   │               │              │
└───────────────┴───────────────┴──────────────┘
```

Modules (single crate, `src/`):

- `config.rs` — parse/validate TOML, file watcher (`notify`), atomic reload.
- `hook.rs` — `WH_MOUSE_LL` low-level hook; on a mapped button event, swallow the original and dispatch an action. Must return fast (hand off to a channel/thread; never block the hook).
- `profiles.rs` — resolve active profile: per-app match (exe name, optionally window title regex) → fallback to global. Per-button fallthrough: an app profile only overrides buttons it defines.
- `foreground.rs` — track foreground process via `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` (event-driven, no polling) and cache exe name.
- `actions.rs` — `SendInput` for keys/combos, mouse button swaps, scroll, media keys, launch program, disable (swallow), passthrough.
- `dpi.rs` — detect DPI changes and publish current value.
- `osd.rs` — borderless, topmost, click-through, layered window; fades in/out after ~1.5 s; drawn with plain GDI (no GPU/UI framework).
- `tray.rs` — tray icon + menu (`tray-icon` crate or raw `Shell_NotifyIconW`).

Suggested crates: `windows` (official bindings), `serde` + `toml`, `notify`, `tray-icon`, `anyhow`. Keep the dependency list short for binary size and audit surface.

## Config sketch

```toml
[settings]
osd_ms = 1500
start_enabled = true

# Global profile: applies everywhere
[global]
side_back    = "key:ctrl+z"
side_forward = "key:ctrl+y"
middle       = "key:ctrl+w"
dpi_button   = "passthrough"

# Per-app overrides (only listed buttons change)
[[app]]
exe = "chrome.exe"
side_back    = "key:alt+left"
side_forward = "key:alt+right"

[[app]]
exe = "Code.exe"
side_back    = "key:ctrl+alt+left"
```

Button names: `left, right, middle, x1 (back), x2 (forward), wheel_up, wheel_down, wheel_left, wheel_right`. Action forms: `key:<combo>`, `mouse:<button>`, `run:<path>`, `none` (swallow), `passthrough`.

## Phases

### Phase 0 — Spike (de-risk DPI first)
- Identify the mouse model and how DPI switching surfaces:
  - Many mice change DPI **in firmware** and send nothing to the OS → we can only detect it via a vendor HID report (if one exists) or not at all.
  - Capture HID reports (e.g. with Wireshark+USBPcap or `hidapi` dump) while pressing the DPI button.
- Outcome: pick a detection strategy (below) or fall back.

### Phase 1 — Global remap + tray (MVP)
- Cargo project, low-level mouse hook, `SendInput` injector.
- TOML config + hot reload, global profile only.
- Tray icon with Enable/Disable, Reload, Open config, Quit.
- Guard against re-injection loops (tag injected events via `dwExtraInfo`).

### Phase 2 — Per-app profiles
- Foreground-window watcher, exe-based matching, per-button fallthrough to global.
- Optional title-regex matching; handle elevated/UWP windows gracefully (note: hooks can't affect elevated windows unless we run elevated — document this).

### Phase 3 — DPI OSD
- Implement the strategy chosen in Phase 0, show OSD with the DPI value; fade-out; position (corner/center) configurable; multi-monitor aware (show on the monitor with the cursor).

### Phase 4 — Polish
- Autostart toggle (registry `Run` key), single-instance guard, config error toasts, installer/portable zip, CI build (GitHub/Forgejo → Windows target), README with config reference.

### Phase 5 — Optional
- Simple GUI config editor, import from XMBC settings, Linux backend (evdev/uinput + X11/Wayland focus tracking).

## DPI detection strategies (Phase 3 detail)

| Strategy | Works when | Cost |
|---|---|---|
| **A. Vendor HID report listener** (`hidapi`/raw input) | Mouse sends a report on DPI change (many Razer/Logitech do via a vendor interface) | Per-device protocol; most accurate |
| **B. Own the DPI button + cycle table** | Mouse allows DPI to be set from host, or DPI button is remappable | We keep a user-defined list of DPI stages (`dpi_stages = [400, 800, 1600, 3200]`) and track index; can drift from hardware state |
| **C. Measure counts-per-distance** | Last resort | Inaccurate, not recommended |

Plan: attempt A for the user's specific mouse; ship B as the generic fallback so the feature works for any mouse.

## Resource targets

- Idle CPU ≈ 0% (event-driven only: hook callbacks, WinEvent, file watcher, no polling loops).
- RAM < 10 MB resident; binary < 2 MB (`opt-level="z"`, `lto`, `panic="abort"`, strip).
- Hook callback < 1 ms; heavy work off the hook thread.

## Risks

1. **DPI visibility** — firmware-only DPI switching may be undetectable; mitigated by strategy B and Phase 0 spike.
2. **Elevated windows** — a non-elevated hook can't remap input into admin windows; document, optionally offer "run as admin".
3. **Anti-cheat / games** — low-level hooks and injected input can trigger anti-cheat; provide a per-app "disable in this app" profile.
4. **Hook timeout** — Windows silently drops slow low-level hooks; keep callback minimal.
5. **Dev environment** — developing in WSL2 but targeting Windows: use `cargo build --target x86_64-pc-windows-gnu` (or build natively on Windows); testing requires running the .exe on Windows.

## Testing

- Unit tests: config parsing/validation, profile resolution (global vs per-app fallthrough), combo parsing.
- Manual checklist: each button/action globally, per-app override, switching foreground apps, hot reload with a broken config (must keep last good), DPI OSD on multi-monitor, tray menu, autostart.

## Questions for you before execution

1. Confirm **Windows** is the target (vs Linux)?
2. Which **mouse model**? (Determines DPI detection feasibility.)
3. Is Rust OK, or do you prefer Go / C# / C++?
4. Which buttons/actions do you map today in X-Mouse Button Control? (Seeds the default config and action list.)

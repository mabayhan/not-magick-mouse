# not-magick-mouse — Plan

A tiny, resource-efficient mouse button remapper with a tray icon, a simple config GUI, and a Razer-style DPI on-screen display. Think "X-Mouse Button Control, minus the bloat".

## Confirmed requirements

- **OS:** Windows 10/11 (dev happens in WSL; cross-compile or build natively on Windows).
- **Mouse:** Razer DeathAdder Elite (replacing Razer Synapse).
- **Language:** Rust.
- **Default mapping:** Back button (X1) → `Ctrl+W`.

## Goals

1. **Global mappings** — remap mouse buttons to keys / combos / actions everywhere.
2. **Per-app mappings** — override the global profile when a specific app is focused.
3. **DPI OSD** — show the current DPI on screen (Razer style) when the DPI button is pressed.
4. **System tray** — runs in the tray with no main window.
5. **Simple GUI** — pick a button, press the keys you want, save. No hand-editing needed.

Non-goals (v1): timed macros / recording, RGB, firmware flashing, cloud sync.

## Design decisions

| Decision | Choice | Why |
|---|---|---|
| Process split | **Two executables/modes:** `nmm` daemon (tray + hook + OSD) and `nmm --gui` (config editor) | Daemon stays ~0% CPU and a few MB RAM. The GUI is spawned from the tray on demand and exits when closed, so its heavier toolkit never costs idle memory. |
| GUI toolkit | **egui/eframe** (glow backend), same binary launched with `--gui` | Pure Rust, tiny code, easy "press a key" capture. Alternative if size/RAM matters more: `slint` or raw Win32. |
| Config | **TOML**, hot-reloaded by the daemon | GUI writes it, daemon notices via file watcher. Still hand-editable and diffable. |
| Writes | Atomic (write temp → rename); daemon keeps last good config if a reload fails | A broken file never kills remapping. |

## Architecture

```
          ┌────────────── nmm daemon (always on) ──────────────┐
          │  tray ── menu: Enable/Disable, Settings…, Quit      │
          │                                                     │
 mouse ──►│ WH_MOUSE_LL hook ─► profile engine ─► action        │──► SendInput /
          │                      ▲  (global +      injector     │    WM_APPCOMMAND
          │       foreground ────┘   per-app)                   │
          │       watcher (WinEvent)                            │
          │                                                     │
 Razer ──►│ DPI monitor (HID) ─► OSD window (GDI, click-through)│
 HID      │                                                     │
          │ config watcher ◄── config.toml ◄── nmm --gui        │
          └─────────────────────────────────────────────────────┘
```

Modules (`src/`):

- `config.rs` — TOML schema, validation, load/save, `notify` watcher, atomic reload.
- `hook.rs` — low-level mouse hook. Swallow mapped buttons (both down and up), dispatch via channel; callback must return in well under 1 ms.
- `profiles.rs` — active profile resolution: per-app match (exe name, optional title regex) → fallback to global, **per button** (an app profile only overrides buttons it defines).
- `foreground.rs` — `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`; event-driven, no polling.
- `actions.rs` — action catalog + executor (see below). Injected events are tagged via `dwExtraInfo` to avoid loops.
- `dpi.rs` — Razer HID listener (see DPI section).
- `osd.rs` — borderless, topmost, click-through layered window with GDI text; fade out after ~1.5 s; appears on the monitor containing the cursor.
- `tray.rs` — `tray-icon` crate.
- `gui/` — egui app (profiles list, button list, action picker, key recorder).

Crates (kept short): `windows`, `serde`, `toml`, `notify`, `tray-icon`, `anyhow`, `eframe`/`egui` (GUI only), `hidapi` (DPI).

## Actions: what Windows offers instead of "Ctrl+W"

Windows has **no universal "close tab" command**. `Ctrl+W` is only a convention that browsers, editors, Explorer and most tabbed apps implement. So the action picker offers named presets (stored as readable names in the config) alongside raw shortcuts:

| Preset | Sends | Notes |
|---|---|---|
| Close tab / document | `Ctrl+W` | De-facto standard; **default for Back button**. Fails in apps that don't use it (e.g. many terminals use `Ctrl+Shift+W`) → handle with a per-app override. |
| Close tab (MDI) | `Ctrl+F4` | Older document-style apps. |
| Close window | `Alt+F4` | Universal window close. |
| Browser back / forward | `WM_APPCOMMAND` `BROWSER_BACKWARD/FORWARD` | OS-level, works without hard-coding per-app shortcuts. |
| Close (app command) | `APPCOMMAND_CLOSE` | Only some apps honour it. |
| Media / volume | Media keys | Play/pause, next, prev, mute, vol ±. |
| Virtual desktop / Task view | `Win+Ctrl+←/→`, `Win+Tab` | |
| Copy / Paste / Undo / Redo | `Ctrl+C/V/Z/Y` | |
| Custom shortcut | Recorded in GUI | Any combo of modifiers + key. |
| Mouse button | Another button | Swap/duplicate buttons. |
| Run program | Path + args | |
| Disabled | Swallow the click | |
| Passthrough | Leave unchanged | |

Per-app overrides make the "tab close" problem a non-issue: global `Ctrl+W`, terminal profile `Ctrl+Shift+W`.

## GUI design

Single small window, opened from tray → "Settings…".

```
┌ Profiles ──────┐ ┌ Buttons: Global ───────────────────────────┐
│ ▸ Global       │ │ Left        passthrough                     │
│   chrome.exe   │ │ Right       passthrough                     │
│   WindowsTerm… │ │ Middle      passthrough                     │
│ [+ Add app]    │ │ Back (X1)   Close tab (Ctrl+W)     [Change] │
│                │ │ Forward(X2) passthrough            [Change] │
│                │ │ DPI button  passthrough            [Change] │
└────────────────┘ └─────────────────────────────────────────────┘
 [Change] → picker: ( ) Preset ▾   ( ) Record shortcut [● press keys…]
```

- **Key recorder:** "Record" arms capture; the user presses a combo; modifiers + final key are shown live and committed on release. Esc cancels. Uses window key events inside the GUI (the app is focused during recording, so no global hook needed).
- **Add app profile:** pick from currently running windows (enumerate top-level windows → exe name + icon) or browse for an `.exe`.
- **Detect mouse button:** "Press a mouse button" capture so users don't have to know which button is X1/X2 (uses the GUI's own mouse events; extra Razer buttons that only appear as HID are out of scope).
- Changes apply on **Save**; daemon hot-reloads. Validation errors shown inline.
- Show a conflict warning when two profiles match the same exe.

## Config sketch

```toml
[settings]
osd_ms = 1500
osd_position = "bottom-center"   # corners / center options
start_with_windows = true

[global]
x1 = "preset:close_tab"          # Back  → Ctrl+W
x2 = "passthrough"

[[app]]
exe = "WindowsTerminal.exe"
x1 = "key:ctrl+shift+w"

[[app]]
exe = "game.exe"
disabled = true                  # daemon does nothing while this app is focused
```

Buttons: `left, right, middle, x1, x2, wheel_up/down/left/right`. Action forms: `preset:<name>`, `key:<combo>`, `mouse:<button>`, `run:<path>`, `none`, `passthrough`.

## DPI OSD for the DeathAdder Elite

The Elite's DPI button changes the sensor DPI **in firmware** (5 stages, up to 16,000 DPI). Windows sees no standard event, so the daemon needs the Razer-specific HID channel. Findings to confirm in the spike:

- Razer devices speak a documented-by-reverse-engineering protocol (see the **OpenRazer** project) over HID feature reports on the mouse's control interface; "get DPI" is a known command, so the current DPI can be **read**, not guessed.
- Razer devices also emit a report on DPI-stage change (which Synapse uses). If the Elite does so, we listen for it (event-driven, no polling) and then issue "get DPI".
- **Synapse conflict:** Synapse may hold the interface or fight over DPI settings. The plan assumes Synapse is uninstalled/stopped; note this in the README.

Strategy order:
1. **Listen for the DPI-change report + read DPI** (preferred, exact).
2. **Fallback — low-rate poll** (e.g. 4 Hz, only while the mouse has recently moved or a button was pressed) if no change event exists. Slightly more CPU, still negligible.
3. **Last resort — user-defined stage table** (`dpi_stages = [...]`) tracking presses of the DPI button; may drift.

This is built for the DeathAdder Elite first; other mice would need their own backend.

## Phases

### Phase 0 — Spike (DPI feasibility)
Enumerate Razer HID interfaces (VID `1532`), dump reports while pressing the DPI button, test "get DPI" feature report. Decide strategy 1/2/3. ~half a day; gate for Phase 4.

### Phase 1 — Core remap + tray (MVP)
Cargo project, low-level hook, `SendInput` injector, TOML config + hot reload, global profile only, tray menu (Enable/Disable, Settings, Reload, Quit). Back → Ctrl+W working end to end.

### Phase 2 — Per-app profiles
Foreground watcher, exe matching (+ optional title regex), per-button fallthrough, "disabled in this app" profile.

### Phase 3 — Config GUI
`nmm --gui`: profile list, button list, action picker with presets, shortcut recorder, running-app picker, mouse-button detect, save/validate. Tray launches it; single-instance guard.

### Phase 4 — DPI OSD
Implement the Phase 0 strategy; OSD window with fade; position/duration settings; multi-monitor.

### Phase 5 — Polish
Autostart (HKCU `Run`), single instance, error toasts, portable zip + installer, CI Windows build, README with config reference and Synapse note.

### Later
Import from XMBC, per-profile DPI presets, more mice, Linux backend.

## Resource targets

- Daemon idle CPU ≈ 0% (events only); RAM < 10 MB; binary < 2 MB (`opt-level="z"`, LTO, `panic="abort"`, strip). The GUI binary can be larger since it only runs on demand — consider shipping the GUI as a separate exe so the daemon file stays tiny.
- Hook callback < 1 ms; work happens off the hook thread.

## Risks

1. **DPI reads:** protocol details unverified for the Elite; mitigated by Phase 0 and the fallback ladder.
2. **Elevated windows:** a non-elevated hook can't inject into admin windows; document, optionally offer "run as admin".
3. **Games/anti-cheat:** hooks + injected input can trip anti-cheat; per-app "disabled" profile.
4. **Hook timeouts:** Windows drops slow low-level hooks; keep the callback minimal.
5. **Dev environment:** developing in WSL but running on Windows; build with `--target x86_64-pc-windows-gnu` (or natively on Windows) and test the `.exe` on the Windows side. HID and hooks can't be tested inside WSL.
6. **Razer driver interplay:** Synapse running alongside may override DPI/buttons.

## Testing

- Unit: config parse/validate, profile resolution (global vs per-app per-button fallthrough), combo parsing, preset expansion.
- Manual checklist: each action type, per-app override, switching foreground apps, hot reload with broken config, GUI recorder (modifier-only, Esc cancel), DPI OSD at each of the 5 stages, multi-monitor, tray menu, autostart.

## Remaining questions

1. Is Razer Synapse still installed? OK to uninstall it for the DPI feature?
2. Do you want the OSD to show only the DPI number (e.g. `1600 DPI`) or a stage bar too (like Synapse)?
3. Any other buttons you plan to remap beyond Back → Ctrl+W (e.g. Forward, DPI button, wheel tilt)? The Elite has only 7 buttons, which is a small set.

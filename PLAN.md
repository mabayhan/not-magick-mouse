# not-magick-mouse — Plan

A tiny, resource-efficient mouse button remapper with a tray icon, a simple config GUI, and a Razer-style DPI on-screen display. Think "X-Mouse Button Control, minus the bloat".

## Confirmed requirements

- **OS:** Windows 10/11 only for now. Linux support comes later, so the code is structured to allow it (see Portability).
- **Mouse:** Razer DeathAdder Elite. Razer Synapse is already uninstalled.
- **Language:** Rust.
- **Default mappings: none.** A fresh install changes no button. The user maps everything through the GUI.
- **DPI OSD:** shows only the DPI value (e.g. `1600 DPI`). No animation, no fade, no stage bar: it appears and disappears after a timeout.

## Goals

1. **Global mappings**: remap mouse buttons to keys, combos or actions everywhere.
2. **Per-app mappings**: override the global profile when a specific app is focused.
3. **DPI OSD**: show the current DPI when the DPI button is pressed.
4. **System tray**: runs in the tray with no main window.
5. **Simple GUI**: pick a button, press the keys you want (or choose a named action), save.

Non-goals (v1): timed macros or recording, RGB, firmware flashing, cloud sync, wheel remapping, window-title matching.

## Design decisions

| Decision | Choice | Why |
|---|---|---|
| Process split | Two executables: `nmm` (daemon: tray, hook, OSD) and `nmm-gui` (config editor) | The daemon stays near 0% CPU and a few MB of RAM. The GUI is launched from the tray on demand and exits when closed. |
| GUI toolkit | `egui`/`eframe` | Pure Rust, small code, easy "press a key" capture. Only the GUI exe carries it. |
| Config | TOML at `%APPDATA%\not-magick-mouse\config.toml` | GUI writes it; still hand-editable and diffable. |
| Config writes | Atomic (write temp file, then rename); daemon keeps the last good config if a reload fails | A broken file never stops remapping. |
| Reload signal | GUI broadcasts a registered window message (`NMM_RELOAD`) to the daemon; tray also has "Reload config" | Zero polling and no file-watcher dependency. |
| Tray | Raw `Shell_NotifyIconW` plus a hidden message window | One fewer dependency; the daemon already needs a Win32 message loop for the hooks. |
| Single instance | Named mutex per exe | Avoids double hooks. |

## Architecture

```
          ┌────────────── nmm daemon (always on) ──────────────┐
          │  tray menu: Enable/Disable, Settings…, Reload, Quit │
          │                                                     │
 mouse ──►│ WH_MOUSE_LL hook ─► profile engine ─► action        │──► SendInput /
          │                      ▲  (global +      injector     │    WM_APPCOMMAND
          │       foreground ────┘   per-app)                   │
          │       watcher (WinEvent)                            │
          │                                                     │
 Razer ──►│ DPI thread (hidapi) ─► OSD window (GDI text only)   │
 HID      │                                                     │
          │ NMM_RELOAD msg ◄── config.toml ◄── nmm-gui          │
          └─────────────────────────────────────────────────────┘
```

Layout: a Cargo workspace.

- `core` (platform-independent, unit-testable anywhere, including WSL): config schema/validation, action model and parsing, key-name tables, profile resolution.
- `windows/platform` (`nmm-platform-windows`): hook, injector, foreground watcher, tray, OSD, Razer HID.
- `windows/daemon` (`nmm`): wires core and platform together.
- `gui` (`nmm-gui`): egui app using `core` for load/save/validate.

Modules:

- `config` (core): TOML schema, validation, load/save.
- `actions` (core): action enum, presets, `Combo` parsing (modifiers + key name to virtual-key code).
- `profiles` (core): `resolve(exe, button) -> Action`. An app profile overrides only the buttons it defines; everything else falls through to global. A profile marked `disabled = true` makes the daemon do nothing in that app.
- `hook` (windows): low-level mouse hook. Swallows mapped buttons (both down and up), tracks swallowed buttons so a foreground change between down and up cannot leave a stuck button, and ignores events tagged as its own via `dwExtraInfo`. The callback must return in well under 1 ms.
- `foreground` (windows): `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` to get the exe name of the focused window. Event-driven, no polling.
- `inject` (windows): `SendInput` for keys and mouse buttons, `WM_APPCOMMAND`, and a process spawn on a separate thread for "run program".
- `dpi` (windows): Razer HID reader (see DPI section).
- `osd` (windows): borderless, topmost, click-through layered window with GDI text; shown on the monitor under the cursor; hidden by a timer after `osd_ms`.
- `tray` (windows): icon and popup menu.

Crates (kept short): `windows`, `serde`, `toml`, `anyhow`, `hidapi` (windows-native backend, no C build), `eframe`/`egui` (GUI only).

## Actions

Windows has no universal "close tab" command. `Ctrl+W` is only a convention that browsers, editors, Explorer and most tabbed apps follow. So the GUI offers named presets alongside raw recorded shortcuts, and per-app overrides cover apps that differ (for example a terminal using `Ctrl+Shift+W`).

| Preset | Sends | Notes |
|---|---|---|
| Close tab / document | `Ctrl+W` | De-facto standard. Not applied by default. |
| Close tab (MDI) | `Ctrl+F4` | Older document-style apps. |
| Close window | `Alt+F4` | Universal window close. |
| Browser back / forward | `WM_APPCOMMAND` | OS-level, no per-app shortcut needed. |
| Media / volume | media keys | Play/pause, next, previous, mute, volume up/down. |
| Virtual desktops / Task view | `Win+Ctrl+Left/Right`, `Win+Tab` | |
| Copy / Paste / Undo / Redo | `Ctrl+C/V/Z/Y` | |
| Custom shortcut | recorded in GUI | Any modifiers plus one key. |
| Mouse button | another button | Swap or duplicate buttons. |
| Run program | path plus args | |
| Disabled | swallow the click | |
| Passthrough | leave unchanged | This is the default for every button. |

## GUI design

A single small window, opened from tray, "Settings…".

```
┌ Profiles ──────┐ ┌ Buttons: Global ───────────────────────────┐
│ ▸ Global       │ │ Left        passthrough                     │
│   chrome.exe   │ │ Right       passthrough                     │
│   WindowsTerm… │ │ Middle      passthrough                     │
│ [+ Add app]    │ │ Back (X1)   passthrough            [Change] │
│                │ │ Forward(X2) passthrough            [Change] │
└────────────────┘ └─────────────────────────────────────────────┘
 [Change] opens: ( ) Preset ▾   ( ) Record shortcut [● press keys…]
```

- **Key recorder:** "Record" arms capture; the user presses a combo; modifiers and the final key are shown live and committed on release. Esc cancels. It uses the GUI window's own key events, so no global hook is needed.
- **Add app profile:** pick from currently running windows (exe name and icon) or browse for an `.exe`.
- **Detect mouse button:** "Press a mouse button" capture, so the user does not need to know which is X1 or X2.
- **Save** validates and writes atomically, then notifies the daemon. Errors show inline.
- A warning appears if two profiles match the same exe.

## Config sketch

Everything is empty by default; the GUI fills it in.

```toml
[settings]
osd_ms = 1500
osd_position = "bottom-center"
dpi_poll_ms = 500          # 0 disables polling-based DPI detection
start_with_windows = false

[global]
# empty: no button is remapped

# example of what the GUI may write:
# [global]
# x1 = "key:ctrl+w"
#
# [[app]]
# exe = "WindowsTerminal.exe"
# x1 = "key:ctrl+shift+w"
#
# [[app]]
# exe = "game.exe"
# disabled = true
```

Buttons: `left, right, middle, x1, x2`. Action forms: `preset:<name>`, `key:<combo>`, `mouse:<button>`, `run:<path>`, `none`, `passthrough`.

## DPI OSD for the DeathAdder Elite

The Elite changes DPI in firmware, so Windows gets no standard event. The daemon reads the value over Razer's HID protocol. This is reverse-engineered (OpenRazer project) and unverified for this mouse, so Phase 0 confirms it:

- Query: a 90-byte Razer report sent as a HID feature report, command class `0x04`, command `0x85` ("get DPI"), with the device's transaction id (believed `0x3F` for the Elite). The reply holds the X/Y DPI.
- Detection ladder:
  1. If the mouse emits an input report on DPI change, listen for it (event-driven) and then query DPI.
  2. Otherwise poll the query at a low rate (default 500 ms, configurable, 0 to disable). The cost is one tiny HID transaction per tick.
  3. Last resort: a user-defined list of DPI stages, tracked by DPI button presses. May drift from the real value.
- Only show the OSD when the value changes after the first read. Retry opening the device every few seconds if it is unplugged.
- The OSD text is just `<value> DPI`. Window is shown, then hidden by a timer. No animation.

## Portability (Linux later)

- Keep all OS calls behind small traits in `core` (`InputHook`, `Injector`, `FocusWatcher`, `Osd`, `Tray`, `DpiSource`). The Windows crate implements them now.
- The config format already uses neutral names (buttons, `key:` combos, exe names). Linux would add a backend using evdev/uinput and X11/Wayland focus tracking, plus a different key-code table; combos are stored by key name, not Windows virtual-key code, to make that possible.
- Nothing in `core` may depend on `windows` types.

## Phases

### Phase 0: Spike (DPI feasibility)
Enumerate Razer HID interfaces (VID `1532`), try the "get DPI" feature report on each, and dump input reports while pressing the DPI button. Choose detection strategy 1, 2 or 3. Gate for Phase 4.

### Phase 1: Core and daemon MVP
Workspace setup, `core` with tests, hook, injector, tray, config load with reload message. With an empty config, nothing is remapped.

### Phase 2: Per-app profiles
Foreground watcher, exe matching, per-button fallthrough, "disabled in this app".

### Phase 3: Config GUI
Profile list, button list, preset and shortcut picker, recorder, running-app picker, mouse-button detect, save, validate, notify daemon. Launched from tray.

### Phase 4: DPI OSD
Implement the Phase 0 strategy and the plain-text OSD.

### Phase 5: Polish
Autostart (HKCU `Run`), error messages for a bad config, portable zip, CI Windows build, README with config reference.

### Later
Linux backend, import from X-Mouse Button Control, wheel/tilt remapping, other mice.

## Build environment note

Development happens in WSL, but this is a Windows program, and this WSL has no C toolchain or sudo. Options, in order of preference:

1. Build natively on Windows (`rustup` with the MSVC target). Hooks, HID and the tray can only be tested there anyway.
2. In WSL, install `mingw-w64` and build `--target x86_64-pc-windows-gnu`; copy the `.exe` to Windows to run it.

Pure `core` tests need a host C linker (`build-essential`) or must run on Windows.

## Resource targets

- Daemon idle CPU near 0% (event-driven; the only timer is the optional DPI poll). RAM under 10 MB. Binary under 2 MB (`opt-level="z"`, LTO, `panic="abort"`, strip).
- Hook callback under 1 ms; slow work (run program) off the hook thread.

## Risks

1. **DPI reads:** protocol details unverified for the Elite; mitigated by Phase 0 and the fallback ladder.
2. **Elevated windows:** a non-elevated hook cannot inject into admin windows. Document it and optionally offer "run as admin".
3. **Games and anti-cheat:** hooks and injected input can trip anti-cheat. Use a per-app "disabled" profile.
4. **Hook timeouts:** Windows drops slow low-level hooks, so the callback stays minimal.
5. **Testing:** hooks, HID and tray cannot be tested in WSL; needs a Windows run.

## Testing

- Unit (core): config parse/validate, profile resolution (fallthrough and disabled), combo parsing, preset expansion, empty config means all passthrough.
- Manual checklist: each action type, per-app override, switching foreground apps, no stuck buttons when focus changes mid-click, reload with a broken config, GUI recorder (modifier-only, Esc cancel), DPI OSD at each stage, multi-monitor, tray menu, autostart.

## Status

- **Phase 0 done.** On the DeathAdder Elite (PID `005C`), HID interface 0 (usage page 1, usage 2) answers the "get DPI" report with transaction id `0x3F` (reply status `0x02`, DPI big-endian at report bytes 10-11). The feature report fails on the other interfaces. Strategy 2 (polling) is implemented; whether the mouse emits an input report on the DPI button was not tested, so the event-driven option is still open.
- **Phases 1-4 implemented**, plus autostart and the README from Phase 5. Release sizes: daemon 0.5 MB, GUI 4.9 MB.
- Verified by automated tests (core, key map, GUI key mapping) and a smoke run (daemon up, single instance enforced, bad config reload survived, GUI opens).
- **Not yet verified by hand:** the manual checklist under Testing (each action type, per-app override, stuck-button check, OSD on a real DPI change, multi-monitor, tray menu, recorder). Daemon RAM is about 17 MB, above the 10 MB target.
- **Not done:** CI workflow, portable zip, a custom tray icon (uses the default application icon), and the `InputHook`/`Injector`-style traits are declared in `core` but the Windows crate does not implement them yet.

## Multiple mice and profile scopes (v0.2)

Two or more mice can be connected at once. Settings are layered by scope.

- **GUI selection model.** The window opens on Global with no mouse selected; every mouse is assumed to have five buttons. Selecting a mouse edits that mouse in every app. Selecting an app keeps the selected mouse, giving mouse + app; with no mouse it is global + app. Clicking the mouse again drops the app, Global clears both.
- **Resolution order** (first layer that defines the button wins; each layer only overrides what it defines): mouse + app, app (any mouse), mouse (any app), Global. Disabled flags exist on the mouse, the app and the mouse-in-app, and turn remapping off for that scope.
- **Config.** `[[device]]` (id = `vid_xxxx&pid_xxxx`, optional `name`, `disabled`, `dpi_stages`, buttons) and `[[app.device]]` nested under an app (id, `disabled`, buttons). Identical mice share an id. Everything is optional, so old configs load unchanged.
- **Which mouse clicked.** The low-level hook cannot tell, so the daemon also registers for Raw Input and keeps a short list of recent button transitions tagged with the device. The hook claims the matching entry (it first drains the thread's raw input buffer so the entry exists even if `WM_INPUT` has not been dispatched). No match means "unknown mouse": only Global and app (any mouse) layers apply, so remapping never depends on this lookup. Risk: relies on Windows queuing raw input before the hook runs; needs real-hardware confirmation.
- **DPI.** One poller thread tracks every Razer mouse (mouse collection interface per device), rescans every 5 s for hot-plug, applies per-mouse stage lists falling back to the global list, and the OSD names the mouse only when more than one is connected.
- **Deployment.** CI on push/PR and a tag-driven release workflow that builds the NSIS installer and portable zip on Linux (`windows/installer/package.sh`), documented in the README.
- **DPI actions (v0.2).** `Action::Dpi(next|prev|N)` acts on the clicked mouse via a dedicated worker thread (no HID I/O on the hook thread). Razer: existing HID feature reports. Logitech: HID++ 2.0 over the vendor collection (usage page FF43), feature 0x2201 `getSensorDpiList/getSensorDpi/setSensorDpi`, verified read and write on an MX Master 2S over BLE. DPI values the worker sets are recorded so the Razer poller does not treat them as a DPI-button press. No DPI action is assigned by default.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use nmm_core::config::{AppProfile, Config, default_config_path, explicit_config_path, resolve_config_path, set_saved_config_path};
use std::path::PathBuf;
use nmm_core::keys::Modifier;
use nmm_core::{Action, Button, Combo, Preset};

fn main() -> eframe::Result {
    let forced = explicit_config_path(std::env::args().skip(1)).is_some();
    let path = resolve_config_path(std::env::args().skip(1));
    if !nmm_platform_windows::single_instance("Local\\not-magick-mouse-gui") {
        return Ok(());
    }
    let icon = egui::IconData { rgba: include_bytes!("../icon64.rgba").to_vec(), width: 64, height: 64 };
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 680.0])
            .with_min_inner_size([640.0, 520.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native("not-magick-mouse", opts, Box::new(|_cc| Ok(Box::new(App::new(path, forced)))))
}

#[derive(Clone, Copy, PartialEq)]
enum Sel {
    Global,
    App(usize),
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Passthrough,
    Disabled,
    Preset,
    Shortcut,
    Mouse,
    Run,
}

impl Kind {
    const ALL: [Kind; 6] = [Kind::Passthrough, Kind::Disabled, Kind::Preset, Kind::Shortcut, Kind::Mouse, Kind::Run];
    fn label(self) -> &'static str {
        match self {
            Kind::Passthrough => "Passthrough (unchanged)",
            Kind::Disabled => "Disabled (swallow click)",
            Kind::Preset => "Preset action",
            Kind::Shortcut => "Keyboard shortcut",
            Kind::Mouse => "Mouse button",
            Kind::Run => "Run program",
        }
    }
}

/// Draft state of the row editor.
struct Editor {
    button: Button,
    kind: Kind,
    preset: Preset,
    combo: Option<Combo>,
    win: bool,
    recording: bool,
    mouse: Button,
    run: String,
}

impl Editor {
    fn from(button: Button, current: Option<&Action>) -> Editor {
        let mut e = Editor {
            button,
            kind: Kind::Passthrough,
            preset: Preset::CloseTab,
            combo: None,
            win: false,
            recording: false,
            mouse: Button::Middle,
            run: String::new(),
        };
        match current {
            None | Some(Action::Passthrough) => {}
            Some(Action::None) => e.kind = Kind::Disabled,
            Some(Action::Preset(p)) => {
                e.kind = Kind::Preset;
                e.preset = *p;
            }
            Some(Action::Key(c)) => {
                e.kind = Kind::Shortcut;
                e.win = c.mods.contains(&Modifier::Win);
                e.combo = Some(Combo { mods: c.mods.iter().copied().filter(|m| *m != Modifier::Win).collect(), key: c.key.clone() });
            }
            Some(Action::Mouse(b)) => {
                e.kind = Kind::Mouse;
                e.mouse = *b;
            }
            Some(Action::Run(r)) => {
                e.kind = Kind::Run;
                e.run = r.clone();
            }
        }
        e
    }

    fn build(&self) -> Result<Action, String> {
        match self.kind {
            Kind::Passthrough => Ok(Action::Passthrough),
            Kind::Disabled => Ok(Action::None),
            Kind::Preset => Ok(Action::Preset(self.preset)),
            Kind::Shortcut => {
                let c = self.combo.as_ref().ok_or("record a shortcut first")?;
                let mut mods = c.mods.clone();
                if self.win {
                    mods.push(Modifier::Win);
                }
                Ok(Action::Key(Combo::new(&mods, &c.key)))
            }
            Kind::Mouse => Ok(Action::Mouse(self.mouse)),
            Kind::Run if self.run.trim().is_empty() => Err("enter a program path".into()),
            Kind::Run => Ok(Action::Run(self.run.trim().to_string())),
        }
    }
}

struct App {
    path: PathBuf,
    path_text: String,
    path_forced: bool,
    cfg: Config,
    sel: Sel,
    editor: Option<Editor>,
    detecting: bool,
    dpi_text: String,
    suspend_sent: Option<std::time::Instant>,
    new_exe: String,
    running: Vec<String>,
    dirty: bool,
    status: String,
    status_err: bool,
}

impl App {
    fn new(path: PathBuf, forced: bool) -> App {
        let (cfg, status, status_err) = match Config::load(&path) {
            Ok(c) => (c, String::new(), false),
            Err(e) => (Config::default(), format!("Could not load config: {e:#}"), true),
        };
        App {
            path_text: path.display().to_string(),
            path_forced: forced,
            path,
            dpi_text: cfg.settings.dpi_stages.iter().map(u32::to_string).collect::<Vec<_>>().join(", "),
            cfg,
            sel: Sel::Global,
            editor: None,
            detecting: false,
            suspend_sent: None,
            new_exe: String::new(),
            running: nmm_platform_windows::list_running_apps(),
            dirty: false,
            status,
            status_err,
        }
    }

    fn map_mut(&mut self) -> &mut nmm_core::config::ButtonMap {
        match self.sel {
            Sel::Global => &mut self.cfg.global,
            Sel::App(i) => &mut self.cfg.apps[i].buttons,
        }
    }

    fn map(&self) -> &nmm_core::config::ButtonMap {
        match self.sel {
            Sel::Global => &self.cfg.global,
            Sel::App(i) => &self.cfg.apps[i].buttons,
        }
    }

    fn save(&mut self) {
        match self.cfg.save(&self.path) {
            Ok(()) => {
                nmm_platform_windows::notify_reload();
                self.dirty = false;
                self.status = "Saved. The daemon has been told to reload.".into();
                self.status_err = false;
            }
            Err(e) => {
                self.status = format!("Not saved: {e:#}");
                self.status_err = true;
            }
        }
    }

    /// Switch to another config file (`None` = the default location). An
    /// existing file is loaded; otherwise the current settings are written to it.
    fn change_path(&mut self, text: Option<&str>) {
        let new = match text.map(|t| t.trim().trim_matches('"')) {
            None | Some("") => default_config_path(),
            Some(t) => PathBuf::from(t),
        };
        let fail = |s: &mut App, m: String| {
            s.status = m;
            s.status_err = true;
        };
        if !new.is_absolute() {
            return fail(self, "Use a full path, e.g. D:\\tools\\nmm\\config.toml".into());
        }
        if new == self.path {
            self.status = "That file is already in use.".into();
            self.status_err = false;
            return;
        }
        let loaded = if new.exists() {
            match Config::load(&new) {
                Ok(c) => Some(c),
                Err(e) => return fail(self, format!("Could not load {}: {e:#}", new.display())),
            }
        } else {
            if let Err(e) = self.cfg.save(&new) {
                return fail(self, format!("Could not create {}: {e:#}", new.display()));
            }
            None
        };
        if let Err(e) = set_saved_config_path(Some(&new)) {
            return fail(self, format!("Could not remember the new path: {e:#}"));
        }
        let note = if let Some(c) = loaded {
            self.cfg = c;
            self.dpi_text = self.cfg.settings.dpi_stages.iter().map(u32::to_string).collect::<Vec<_>>().join(", ");
            self.sel = Sel::Global;
            self.editor = None;
            self.dirty = false;
            "loaded the existing file"
        } else {
            "created it from your current settings"
        };
        self.path = new;
        self.path_text = self.path.display().to_string();
        nmm_platform_windows::notify_reload();
        self.status = format!("Config file changed ({note}). The daemon has been told to reload.");
        self.status_err = false;
    }

    fn add_app(&mut self, exe: &str) {
        let exe = exe.trim();
        if exe.is_empty() {
            return;
        }
        if let Some(i) = self.cfg.apps.iter().position(|a| a.exe.eq_ignore_ascii_case(exe)) {
            self.sel = Sel::App(i);
            return;
        }
        self.cfg.apps.push(AppProfile { exe: exe.to_string(), ..Default::default() });
        self.sel = Sel::App(self.cfg.apps.len() - 1);
        self.editor = None;
        self.dirty = true;
    }
}

/// "800, 1600 2400" -> [800, 1600, 2400]; empty -> [].
fn parse_dpi_list(s: &str) -> Result<Vec<u32>, String> {
    let mut out = Vec::new();
    for tok in s.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|t| !t.is_empty()) {
        let v: u32 = tok.parse().map_err(|_| format!("'{tok}' is not a number"))?;
        if !(100..=30000).contains(&v) {
            return Err(format!("{v} is out of range (100-30000)"));
        }
        out.push(v);
    }
    Ok(out)
}

fn describe(a: Option<&Action>) -> String {
    match a {
        None | Some(Action::Passthrough) => "passthrough".into(),
        Some(Action::None) => "disabled".into(),
        Some(Action::Preset(p)) => p.label().into(),
        Some(Action::Key(c)) => format!("shortcut {c}"),
        Some(Action::Mouse(b)) => format!("mouse {}", b.label()),
        Some(Action::Run(r)) => format!("run {r}"),
    }
}

/// egui key -> neutral key name. `None` for keys we don't support.
fn key_name(k: egui::Key) -> Option<String> {
    let n = k.name();
    let lower = n.to_ascii_lowercase();
    let out = if n.len() == 1 && n.as_bytes()[0].is_ascii_alphabetic() {
        lower
    } else if let Some(d) = n.strip_prefix("Num") {
        d.to_string()
    } else {
        match n {
            "Down" | "ArrowDown" => "down",
            "Up" | "ArrowUp" => "up",
            "Left" | "ArrowLeft" => "left",
            "Right" | "ArrowRight" => "right",
            "Enter" => "enter",
            "Tab" => "tab",
            "Space" => "space",
            "Backspace" => "backspace",
            "Delete" => "delete",
            "Insert" => "insert",
            "Home" => "home",
            "End" => "end",
            "PageUp" => "pageup",
            "PageDown" => "pagedown",
            "Minus" => "minus",
            "Equals" => "equals",
            "Comma" => "comma",
            "Period" => "period",
            "Slash" => "slash",
            "Backslash" => "backslash",
            "Semicolon" => "semicolon",
            "Quote" => "quote",
            "Backtick" => "backtick",
            "OpenBracket" => "lbracket",
            "CloseBracket" => "rbracket",
            _ => &lower,
        }
        .to_string()
    };
    nmm_core::keys::is_valid_key(&out).then_some(out)
}

fn pointer_button(b: egui::PointerButton) -> Button {
    match b {
        egui::PointerButton::Primary => Button::Left,
        egui::PointerButton::Secondary => Button::Right,
        egui::PointerButton::Middle => Button::Middle,
        egui::PointerButton::Extra1 => Button::X1,
        egui::PointerButton::Extra2 => Button::X2,
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ---- capture: shortcut recorder and mouse-button detector ----
        let events = ctx.input(|i| i.events.clone());
        for ev in &events {
            match ev {
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    if let Some(ed) = self.editor.as_mut().filter(|e| e.recording) {
                        if *key == egui::Key::Escape {
                            ed.recording = false;
                        } else if let Some(name) = key_name(*key) {
                            let mut mods = Vec::new();
                            if modifiers.ctrl {
                                mods.push(Modifier::Ctrl);
                            }
                            if modifiers.shift {
                                mods.push(Modifier::Shift);
                            }
                            if modifiers.alt {
                                mods.push(Modifier::Alt);
                            }
                            ed.combo = Some(Combo::new(&mods, &name));
                            ed.recording = false;
                        }
                    }
                }
                egui::Event::PointerButton { button, pressed: true, .. } if self.detecting => {
                    let b = pointer_button(*button);
                    self.detecting = false;
                    self.editor = Some(Editor::from(b, self.map().get(b)));
                }
                _ => {}
            }
        }

        // While detecting, ask the daemon to pass every button through (it
        // would otherwise swallow mapped ones). Renewed often; the daemon
        // also auto-resumes if we vanish.
        if self.detecting {
            if self.suspend_sent.is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(2)) {
                nmm_platform_windows::set_remap_suspended(true);
                self.suspend_sent = Some(std::time::Instant::now());
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
        } else if self.suspend_sent.take().is_some() {
            nmm_platform_windows::set_remap_suspended(false);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        nmm_platform_windows::set_remap_suspended(false);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::Panel::left("profiles").default_size(200.0).show(ui, |ui| {
            ui.heading("Profiles");
            ui.separator();
            ui.selectable_value(&mut self.sel, Sel::Global, "Global");
            let mut remove = None;
            for i in 0..self.cfg.apps.len() {
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.sel == Sel::App(i), &self.cfg.apps[i].exe).clicked() {
                        self.sel = Sel::App(i);
                        self.editor = None;
                    }
                    if ui.small_button("x").on_hover_text("Remove profile").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                self.cfg.apps.remove(i);
                self.sel = Sel::Global;
                self.editor = None;
                self.dirty = true;
            }
            ui.separator();
            ui.label("Add app profile");
            let mut pick = None;
            egui::ComboBox::from_id_salt("running").selected_text("Running apps...").show_ui(ui, |ui| {
                for r in &self.running {
                    if ui.selectable_label(false, r).clicked() {
                        pick = Some(r.clone());
                    }
                }
            });
            if ui.small_button("Refresh list").clicked() {
                self.running = nmm_platform_windows::list_running_apps();
            }
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.new_exe).hint_text("name.exe").desired_width(110.0));
                if ui.button("Add").clicked() {
                    pick = Some(std::mem::take(&mut self.new_exe));
                }
            });
            if let Some(p) = pick {
                self.add_app(&p);
            }
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let label = egui::RichText::new("  Save  ").size(18.0).strong().color(egui::Color32::WHITE);
                let fill = if self.dirty { egui::Color32::from_rgb(0, 120, 215) } else { egui::Color32::from_gray(70) };
                let btn = egui::Button::new(label).fill(fill).min_size(egui::vec2(120.0, 36.0));
                if ui.add_enabled(self.dirty, btn).clicked() {
                    self.save();
                }
                if self.dirty {
                    ui.colored_label(egui::Color32::from_rgb(255, 200, 80), "Unsaved changes");
                }
                if !self.status.is_empty() {
                    let color = if self.status_err { egui::Color32::LIGHT_RED } else { egui::Color32::LIGHT_GREEN };
                    ui.colored_label(color, &self.status);
                }
            });
            ui.small(format!("Config file: {}", self.path.display()));
            for w in self.cfg.warnings() {
                ui.colored_label(egui::Color32::YELLOW, format!("Warning: {w}"));
            }
        });

        egui::CentralPanel::default().show(ui, |ui| {
            let title = match self.sel {
                Sel::Global => "Buttons: Global".to_string(),
                Sel::App(i) => format!("Buttons: {}", self.cfg.apps[i].exe),
            };
            ui.heading(title);
            if let Sel::App(i) = self.sel {
                let mut d = self.cfg.apps[i].disabled;
                if ui.checkbox(&mut d, "Disabled in this app (do nothing, all buttons pass through)").changed() {
                    self.cfg.apps[i].disabled = d;
                    self.dirty = true;
                }
                ui.label("Buttons left as passthrough here fall back to the Global profile.");
            } else {
                ui.label("A fresh install changes nothing. Pick a button and press Change.");
            }
            ui.separator();

            let mut open = None;
            for b in Button::ALL {
                ui.horizontal(|ui| {
                    ui.add_sized([110.0, 20.0], egui::Label::new(b.label()));
                    ui.add_sized([280.0, 20.0], egui::Label::new(describe(self.map().get(b))));
                    if ui.button("Change").clicked() {
                        open = Some(b);
                    }
                });
            }
            if let Some(b) = open {
                self.detecting = false;
                self.editor = Some(Editor::from(b, self.map().get(b)));
            }

            ui.add_space(10.0);
            ui.separator();
            ui.strong("Custom DPI values");
            ui.label("The DPI button cycles through these, in order. Leave empty to keep the mouse's own stages.");
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut self.dpi_text).hint_text("800, 1600, 2400, 3200").desired_width(320.0));
                if r.changed() {
                    match parse_dpi_list(&self.dpi_text) {
                        Ok(v) => {
                            if v != self.cfg.settings.dpi_stages {
                                self.cfg.settings.dpi_stages = v;
                                self.dirty = true;
                            }
                            self.status.clear();
                        }
                        Err(e) => {
                            self.status = e;
                            self.status_err = true;
                        }
                    }
                }
                ui.label("DPI");
            });

            ui.add_space(10.0);
            ui.separator();
            ui.strong("Config file");
            if self.path_forced {
                ui.label(format!("{}\nSet by --config or NMM_CONFIG for this session; start without it to choose a path here.", self.path.display()));
            } else {
                let mut action: Option<Option<String>> = None;
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.path_text).desired_width(380.0));
                    if ui.button("Apply").clicked() {
                        action = Some(Some(self.path_text.clone()));
                    }
                    if ui.button("Default").on_hover_text("Use ~/.config/not-magick-mouse/config.toml").clicked() {
                        action = Some(None);
                    }
                });
                ui.small("An existing file is loaded; a new path is created from your current settings. The daemon follows automatically.");
                if let Some(a) = action {
                    self.change_path(a.as_deref());
                }
            }

            ui.add_space(10.0);
            ui.separator();
            if ui.selectable_label(self.detecting, "Detect a mouse button...").clicked() {
                self.detecting = !self.detecting;
            }
            if self.detecting {
                ui.label("Click the mouse button you want to configure (Esc to cancel in the keyboard).");
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.detecting = false;
                }
            }

            let mut apply: Option<(Button, Action)> = None;
            let mut close = false;
            if let Some(ed) = self.editor.as_mut() {
                ui.separator();
                ui.strong(format!("Edit: {}", ed.button.label()));
                egui::ComboBox::from_id_salt("kind").selected_text(ed.kind.label()).show_ui(ui, |ui| {
                    for k in Kind::ALL {
                        ui.selectable_value(&mut ed.kind, k, k.label());
                    }
                });
                match ed.kind {
                    Kind::Preset => {
                        egui::ComboBox::from_id_salt("preset").selected_text(ed.preset.label()).show_ui(ui, |ui| {
                            for p in Preset::ALL {
                                ui.selectable_value(&mut ed.preset, p, p.label());
                            }
                        });
                    }
                    Kind::Shortcut => {
                        ui.horizontal(|ui| {
                            let text = if ed.recording {
                                "press keys... (Esc cancels)".to_string()
                            } else {
                                match &ed.combo {
                                    Some(c) => c.to_string(),
                                    None => "none recorded".to_string(),
                                }
                            };
                            ui.label(text);
                            if ui.button(if ed.recording { "Cancel" } else { "Record" }).clicked() {
                                ed.recording = !ed.recording;
                            }
                            ui.checkbox(&mut ed.win, "+ Win");
                        });
                        ui.small("Windows reserves many Win+key combos, so tick \"+ Win\" instead of pressing the Win key.");
                    }
                    Kind::Mouse => {
                        egui::ComboBox::from_id_salt("mouse").selected_text(ed.mouse.label()).show_ui(ui, |ui| {
                            for b in Button::ALL {
                                ui.selectable_value(&mut ed.mouse, b, b.label());
                            }
                        });
                    }
                    Kind::Run => {
                        ui.add(egui::TextEdit::singleline(&mut ed.run).hint_text("C:\\path\\app.exe args").desired_width(380.0));
                    }
                    Kind::Passthrough | Kind::Disabled => {}
                }
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        match ed.build() {
                            Ok(a) => apply = Some((ed.button, a)),
                            Err(e) => {
                                self.status = e;
                                self.status_err = true;
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            }
            if let Some((b, a)) = apply {
                self.map_mut().set(b, a);
                self.dirty = true;
                self.status.clear();
                close = true;
            }
            if close {
                self.editor = None;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_list_parsing() {
        assert_eq!(parse_dpi_list("800, 1600 2400").unwrap(), vec![800, 1600, 2400]);
        assert!(parse_dpi_list("").unwrap().is_empty());
        assert!(parse_dpi_list("abc").is_err());
        assert!(parse_dpi_list("50").is_err());
    }

    #[test]
    fn common_keys_map() {
        for (k, n) in [
            (egui::Key::A, "a"),
            (egui::Key::Num5, "5"),
            (egui::Key::F4, "f4"),
            (egui::Key::ArrowLeft, "left"),
            (egui::Key::PageDown, "pagedown"),
            (egui::Key::Tab, "tab"),
            (egui::Key::Space, "space"),
        ] {
            assert_eq!(key_name(k).as_deref(), Some(n), "{k:?}");
        }
    }
}

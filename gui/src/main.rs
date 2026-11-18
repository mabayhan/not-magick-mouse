#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod mouse_art;
mod theme;

use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, vec2};
use nmm_core::config::{AppProfile, ButtonMap, Config, DeviceProfile, default_config_path, explicit_config_path, resolve_config_path, set_saved_config_path};
use nmm_core::keys::Modifier;
use nmm_core::{Action, Button, Combo, Preset};
use nmm_platform_windows::MouseInfo;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use theme::*;

fn main() -> eframe::Result {
    let forced = explicit_config_path(std::env::args().skip(1)).is_some();
    let path = resolve_config_path(std::env::args().skip(1));
    if !nmm_platform_windows::single_instance("Local\\not-magick-mouse-gui") {
        return Ok(());
    }
    let icon = egui::IconData { rgba: include_bytes!("../icon64.rgba").to_vec(), width: 64, height: 64 };
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1060.0, 740.0])
            .with_min_inner_size([920.0, 620.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "not-magick-mouse",
        opts,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(App::new(path, forced)))
        }),
    )
}

#[derive(Clone, PartialEq)]
enum Sel {
    Global,
    /// Hardware key of a mouse, e.g. `vid_1532&pid_005c`.
    Device(String),
    App(usize),
    Settings,
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
            Kind::Passthrough => "Unchanged",
            Kind::Disabled => "Block click",
            Kind::Preset => "Preset action",
            Kind::Shortcut => "Keyboard shortcut",
            Kind::Mouse => "Other mouse button",
            Kind::Run => "Run program",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Kind::Passthrough => "The button does what it normally does.",
            Kind::Disabled => "The click is swallowed and does nothing.",
            Kind::Preset => "Common actions: close tab, back, volume, copy and paste...",
            Kind::Shortcut => "Press the keys you want this button to send.",
            Kind::Mouse => "Make this button act like a different mouse button.",
            Kind::Run => "Start a program each time the button is pressed.",
        }
    }
}

/// Draft state of the button editor.
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
                let c = self.combo.as_ref().ok_or("Record a shortcut first.")?;
                let mut mods = c.mods.clone();
                if self.win {
                    mods.push(Modifier::Win);
                }
                Ok(Action::Key(Combo::new(&mods, &c.key)))
            }
            Kind::Mouse => Ok(Action::Mouse(self.mouse)),
            Kind::Run if self.run.trim().is_empty() => Err("Enter a program path.".into()),
            Kind::Run => Ok(Action::Run(self.run.trim().to_string())),
        }
    }
}

static EMPTY_MAP: ButtonMap = ButtonMap { left: None, right: None, middle: None, x1: None, x2: None };
const MICE_REFRESH: Duration = Duration::from_secs(3);

struct App {
    path: PathBuf,
    path_text: String,
    path_forced: bool,
    cfg: Config,
    sel: Sel,
    editor: Option<Editor>,
    detecting: bool,
    /// Which page `dpi_text` belongs to.
    dpi_for: Option<Sel>,
    dpi_text: String,
    suspend_sent: Option<Instant>,
    new_exe: String,
    running: Vec<String>,
    adding_app: bool,
    mice: Vec<MouseInfo>,
    mice_at: Instant,
    dirty: bool,
    status: String,
    status_err: bool,
    frames: u32,
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
            cfg,
            sel: Sel::Global,
            editor: None,
            detecting: false,
            dpi_for: None,
            dpi_text: String::new(),
            suspend_sent: None,
            new_exe: String::new(),
            running: Vec::new(),
            adding_app: false,
            mice: nmm_platform_windows::list_mice(),
            mice_at: Instant::now(),
            dirty: false,
            status,
            status_err,
            frames: 0,
        }
        .with_start_page()
    }

    /// `NMM_GUI_PAGE` opens a given page (used to take the README screenshots):
    /// `global`, `settings`, `mouse:<index or id>`, `app:<n>`, optionally followed by `+edit:<button>`.
    fn with_start_page(mut self) -> App {
        let Ok(v) = std::env::var("NMM_GUI_PAGE") else { return self };
        let (page, edit) = v.split_once("+edit:").map_or((v.as_str(), None), |(p, e)| (p, Some(e)));
        self.sel = match page.split_once(':') {
            Some(("mouse", n)) => Sel::Device(n.parse::<usize>().ok().and_then(|n| self.mice.get(n)).map_or(n.to_string(), |m| m.key.clone())),
            Some(("app", n)) => n.parse::<usize>().ok().filter(|n| *n < self.cfg.apps.len()).map_or(Sel::Global, Sel::App),
            _ if page == "settings" => Sel::Settings,
            _ => Sel::Global,
        };
        if let Some(b) = edit.and_then(|e| e.parse::<Button>().ok()) {
            self.editor = Some(Editor::from(b, self.map().get(b)));
        }
        self
    }

    fn note(&mut self, msg: impl Into<String>, err: bool) {
        self.status = msg.into();
        self.status_err = err;
    }

    /// Index of the profile for a mouse, creating it on first use.
    fn ensure_device(&mut self, key: &str) -> usize {
        if let Some(i) = self.cfg.devices.iter().position(|d| d.id.eq_ignore_ascii_case(key)) {
            return i;
        }
        let name = self.mice.iter().find(|m| m.key == key).map(|m| m.name.clone()).unwrap_or_default();
        self.cfg.devices.push(DeviceProfile { id: key.to_string(), name, ..Default::default() });
        self.cfg.devices.len() - 1
    }

    fn map_mut(&mut self) -> &mut ButtonMap {
        match self.sel.clone() {
            Sel::Device(k) => {
                let i = self.ensure_device(&k);
                &mut self.cfg.devices[i].buttons
            }
            Sel::App(i) => &mut self.cfg.apps[i].buttons,
            _ => &mut self.cfg.global,
        }
    }

    fn map(&self) -> &ButtonMap {
        match &self.sel {
            Sel::Device(k) => self.cfg.device(k).map(|d| &d.buttons).unwrap_or(&EMPTY_MAP),
            Sel::App(i) => &self.cfg.apps[*i].buttons,
            _ => &self.cfg.global,
        }
    }

    fn mouse_name(&self, key: &str) -> String {
        self.mice
            .iter()
            .find(|m| m.key == key)
            .map(|m| m.name.clone())
            .or_else(|| self.cfg.device(key).map(|d| d.name.clone()).filter(|n| !n.is_empty()))
            .unwrap_or_else(|| key.to_string())
    }

    fn save(&mut self) {
        match self.cfg.save(&self.path) {
            Ok(()) => {
                nmm_platform_windows::notify_reload();
                self.dirty = false;
                self.note("Saved. The tray app reloaded your settings.", false);
            }
            Err(e) => self.note(format!("Not saved: {e:#}"), true),
        }
    }

    /// Switch to another config file (`None` = the default location). An
    /// existing file is loaded; otherwise the current settings are written to it.
    fn change_path(&mut self, text: Option<&str>) {
        let new = match text.map(|t| t.trim().trim_matches('"')) {
            None | Some("") => default_config_path(),
            Some(t) => PathBuf::from(t),
        };
        if !new.is_absolute() {
            return self.note("Use a full path, e.g. D:\\tools\\nmm\\config.toml", true);
        }
        if new == self.path {
            return self.note("That file is already in use.", false);
        }
        let loaded = if new.exists() {
            match Config::load(&new) {
                Ok(c) => Some(c),
                Err(e) => return self.note(format!("Could not load {}: {e:#}", new.display()), true),
            }
        } else {
            if let Err(e) = self.cfg.save(&new) {
                return self.note(format!("Could not create {}: {e:#}", new.display()), true);
            }
            None
        };
        if let Err(e) = set_saved_config_path(Some(&new)) {
            return self.note(format!("Could not remember the new path: {e:#}"), true);
        }
        let how = if let Some(c) = loaded {
            self.cfg = c;
            self.sel = Sel::Global;
            self.editor = None;
            self.dpi_for = None;
            self.dirty = false;
            "loaded the existing file"
        } else {
            "created it from your current settings"
        };
        self.path = new;
        self.path_text = self.path.display().to_string();
        nmm_platform_windows::notify_reload();
        self.note(format!("Config file changed ({how})."), false);
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

    fn refresh_mice(&mut self) {
        self.mice = nmm_platform_windows::list_mice();
        self.mice_at = Instant::now();
    }

    // ---------- sidebar ----------

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("not-magick-mouse").strong().size(18.0));
        });
        ui.add_space(14.0);

        group_label(ui, "EVERYTHING");
        if nav_item(ui, self.sel == Sel::Global, "Global", "All mice, all apps", None).clicked() {
            self.sel = Sel::Global;
            self.editor = None;
        }

        ui.add_space(10.0);
        group_label(ui, "MICE");
        let mut keys: Vec<String> = self.mice.iter().map(|m| m.key.clone()).collect();
        for d in &self.cfg.devices {
            if !keys.iter().any(|k| k.eq_ignore_ascii_case(&d.id)) {
                keys.push(d.id.clone());
            }
        }
        if keys.is_empty() {
            ui.label(muted("No mouse found"));
        }
        for k in keys {
            let connected = self.mice.iter().find(|m| m.key == k);
            let has_profile = self.cfg.device(&k).is_some_and(|d| d.buttons != ButtonMap::default() || d.disabled || !d.dpi_stages.is_empty());
            let sub = match (connected.is_some(), has_profile) {
                (true, true) => "Connected - has settings",
                (true, false) => "Connected",
                (false, _) => "Not connected",
            };
            let dot = if connected.is_some() { OK } else { MUTED.gamma_multiply(0.6) };
            let name = self.mouse_name(&k);
            if nav_item(ui, self.sel == Sel::Device(k.clone()), &name, sub, Some(dot)).clicked() {
                self.sel = Sel::Device(k);
                self.editor = None;
            }
        }

        ui.add_space(10.0);
        group_label(ui, "APPS");
        let mut remove = None;
        for i in 0..self.cfg.apps.len() {
            let a = &self.cfg.apps[i];
            let sub = if a.disabled { "Remapping off" } else { "Overrides" };
            let (resp, x) = nav_item_removable(ui, self.sel == Sel::App(i), &a.exe, sub);
            if x {
                remove = Some(i);
            } else if resp.clicked() {
                self.sel = Sel::App(i);
                self.editor = None;
            }
        }
        if let Some(i) = remove {
            self.cfg.apps.remove(i);
            self.sel = Sel::Global;
            self.editor = None;
            self.dirty = true;
        }
        if ui.add(egui::Button::new(RichText::new("+  Add app").color(ACCENT)).frame(false)).clicked() {
            self.adding_app = !self.adding_app;
            if self.adding_app {
                self.running = nmm_platform_windows::list_running_apps();
            }
        }
        if self.adding_app {
            let mut pick = None;
            theme::card(ui, |ui| {
                ui.label(muted("Pick a running app"));
                egui::ScrollArea::vertical().max_height(150.0).id_salt("running").show(ui, |ui| {
                    for r in &self.running {
                        if ui.selectable_label(false, r).clicked() {
                            pick = Some(r.clone());
                        }
                    }
                });
                ui.label(muted("or type its file name"));
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut self.new_exe).hint_text("name.exe").desired_width(110.0));
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("Add").clicked() || enter {
                        pick = Some(std::mem::take(&mut self.new_exe));
                    }
                });
            });
            if let Some(p) = pick {
                self.add_app(&p);
                self.adding_app = false;
            }
        }

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(6.0);
            if nav_item(ui, self.sel == Sel::Settings, "Settings", "Overlay, config file", None).clicked() {
                self.sel = Sel::Settings;
                self.editor = None;
            }
        });
    }

    // ---------- pages ----------

    fn header(&mut self, ui: &mut egui::Ui, title: &str, subtitle: &str, detect: bool) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(title).heading().strong());
                ui.label(muted(subtitle));
            });
            if detect {
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    let label = if self.detecting { "Waiting for a click..." } else { "Detect a button" };
                    if ui.selectable_label(self.detecting, label).on_hover_text("Click the physical button you want to configure").clicked() {
                        self.detecting = !self.detecting;
                    }
                });
            }
        });
        if self.detecting {
            ui.label(RichText::new("Click the mouse button you want to configure, on this window. Esc cancels.").color(ACCENT));
        }
        ui.add_space(4.0);
    }

    fn buttons_card(&mut self, ui: &mut egui::Ui) -> Option<Button> {
        let slots: Vec<mouse_art::Slot> = Button::ALL
            .iter()
            .map(|&b| {
                let a = self.map().get(b);
                mouse_art::Slot { button: b, text: describe(a), mapped: a.is_some() }
            })
            .collect();
        let sel = self.editor.as_ref().map(|e| e.button);
        theme::card(ui, |ui| {
            ui.label(muted("Click a button to change what it does."));
            mouse_art::show(ui, &slots, sel)
        })
    }

    fn dpi_card(&mut self, ui: &mut egui::Ui, title: &str, subtitle: &str, key: Option<&str>) {
        if self.dpi_for.as_ref() != Some(&self.sel) {
            let list = match key {
                Some(k) => self.cfg.device(k).map(|d| d.dpi_stages.clone()).unwrap_or_default(),
                None => self.cfg.settings.dpi_stages.clone(),
            };
            self.dpi_text = list.iter().map(u32::to_string).collect::<Vec<_>>().join(", ");
            self.dpi_for = Some(self.sel.clone());
        }
        theme::card(ui, |ui| {
            section_title(ui, title, subtitle);
            let mut parsed = None;
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut self.dpi_text).hint_text("800, 1600, 2400, 3200").desired_width(320.0));
                ui.label(muted("DPI"));
                if r.changed() {
                    parsed = Some(parse_dpi_list(&self.dpi_text));
                }
            });
            match parsed {
                Some(Ok(v)) => {
                    let cur: &[u32] = match key {
                        Some(k) => self.cfg.device(k).map(|d| d.dpi_stages.as_slice()).unwrap_or(&[]),
                        None => &self.cfg.settings.dpi_stages,
                    };
                    if cur != v.as_slice() {
                        match key {
                            Some(k) => {
                                let i = self.ensure_device(k);
                                self.cfg.devices[i].dpi_stages = v;
                            }
                            None => self.cfg.settings.dpi_stages = v,
                        }
                        self.dirty = true;
                    }
                    self.status.clear();
                }
                Some(Err(e)) => self.note(e, true),
                None => {}
            }
        });
    }

    fn page_global(&mut self, ui: &mut egui::Ui) -> Option<Button> {
        self.header(ui, "Global", "Applies to every mouse in every app, unless a more specific profile overrides it.", true);
        let clicked = self.buttons_card(ui);
        self.dpi_card(
            ui,
            "Default DPI stages",
            "The DPI button of a Razer mouse cycles through these, in order. Leave empty to keep the mouse's own stages.",
            None,
        );
        clicked
    }

    fn page_device(&mut self, ui: &mut egui::Ui, key: String) -> Option<Button> {
        let info = self.mice.iter().find(|m| m.key == key).cloned();
        let name = self.mouse_name(&key);
        let sub = if info.is_some() { "Connected. These buttons only change for this mouse." } else { "Not connected right now. Its settings are kept." };
        self.header(ui, &name, sub, true);
        let disabled = self.cfg.device(&key).is_some_and(|d| d.disabled);
        let clicked = if disabled { None } else { self.buttons_card(ui) };
        theme::card(ui, |ui| {
            let mut d = disabled;
            if ui.checkbox(&mut d, "Ignore this mouse (every button works as normal)").changed() {
                let i = self.ensure_device(&key);
                self.cfg.devices[i].disabled = d;
                self.dirty = true;
            }
            ui.label(muted(format!("Hardware id: {key}. Identical mice share one profile. Buttons you leave unchanged fall back to Global.")));
        });
        if info.as_ref().is_some_and(|m| m.dpi_capable) || key.starts_with("vid_1532") {
            self.dpi_card(ui, "DPI stages for this mouse", "Overrides the default list. Leave empty to use the default list.", Some(&key));
        }
        clicked
    }

    fn page_app(&mut self, ui: &mut egui::Ui, i: usize) -> Option<Button> {
        let exe = self.cfg.apps[i].exe.clone();
        self.header(ui, &exe, "Applies while this app is focused. Buttons left unchanged fall back to the mouse and Global profiles.", true);
        let off = self.cfg.apps[i].disabled;
        let clicked = if off { None } else { self.buttons_card(ui) };
        theme::card(ui, |ui| {
            let mut d = off;
            if ui.checkbox(&mut d, "Turn remapping off in this app (good for games and anti-cheat)").changed() {
                self.cfg.apps[i].disabled = d;
                self.dirty = true;
            }
        });
        clicked
    }

    fn page_settings(&mut self, ui: &mut egui::Ui) {
        self.header(ui, "Settings", "How the tray app behaves.", false);
        theme::card(ui, |ui| {
            section_title(ui, "DPI overlay", "Shown when you press the DPI button.");
            let s = &mut self.cfg.settings;
            let mut ch = false;
            ui.horizontal(|ui| {
                ui.label("Visible for");
                ui.spacing_mut().slider_width = 260.0;
                ch |= ui.add(egui::Slider::new(&mut s.osd_ms, 300..=5000).suffix(" ms")).changed();
            });
            ui.horizontal(|ui| {
                ui.label("Position");
                egui::ComboBox::from_id_salt("osdpos").selected_text(position_label(&s.osd_position)).show_ui(ui, |ui| {
                    for p in POSITIONS {
                        ch |= ui.selectable_value(&mut s.osd_position, p.to_string(), position_label(p)).changed();
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label("Check DPI every");
                ch |= ui.add(egui::DragValue::new(&mut s.dpi_poll_ms).range(0..=5000).speed(25).suffix(" ms")).changed();
                ui.label(muted("0 turns DPI handling off"));
            });
            if ch {
                self.dirty = true;
            }
        });
        theme::card(ui, |ui| {
            section_title(ui, "Startup", "");
            if ui.checkbox(&mut self.cfg.settings.start_with_windows, "Start with Windows").changed() {
                self.dirty = true;
            }
        });
        theme::card(ui, |ui| {
            section_title(ui, "Config file", "Plain TOML, safe to edit by hand.");
            if self.path_forced {
                ui.label(self.path.display().to_string());
                ui.label(muted("Set by --config or NMM_CONFIG for this session; start without it to choose a path here."));
            } else {
                let mut action: Option<Option<String>> = None;
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.path_text).desired_width(440.0));
                    if ui.button("Apply").clicked() {
                        action = Some(Some(self.path_text.clone()));
                    }
                    if ui.button("Default").on_hover_text("Use ~/.config/not-magick-mouse/config.toml").clicked() {
                        action = Some(None);
                    }
                });
                ui.label(muted("An existing file is loaded; a new path is created from your current settings."));
                if let Some(a) = action {
                    self.change_path(a.as_deref());
                }
            }
        });
        ui.label(muted(format!("not-magick-mouse {}", env!("CARGO_PKG_VERSION"))));
    }

    // ---------- editor dialog ----------

    fn editor_dialog(&mut self, ctx: &egui::Context) {
        let name = match &self.sel {
            Sel::Device(k) => format!(" ({})", self.mouse_name(k)),
            Sel::App(i) => format!(" ({})", self.cfg.apps[*i].exe),
            _ => String::new(),
        };
        let Some(ed) = self.editor.as_mut() else { return };
        let mut apply: Option<(Button, Action)> = None;
        let mut close = false;
        let mut err = None;
        let modal = egui::Modal::new(egui::Id::unique("editor"))
            .frame(egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0, BORDER)).corner_radius(14).inner_margin(egui::Margin::same(24)))
            .show(ctx, |ui| {
            ui.set_width(480.0);
            ui.label(RichText::new(format!("{}{name}", ed.button.label())).heading().strong());
            ui.label(muted("What should this button do?"));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for k in Kind::ALL {
                    ui.selectable_value(&mut ed.kind, k, k.label());
                }
            });
            ui.label(muted(ed.kind.hint()));
            ui.add_space(2.0);
            match ed.kind {
                Kind::Preset => {
                    egui::ComboBox::from_id_salt("preset").width(300.0).selected_text(ed.preset.label()).show_ui(ui, |ui| {
                        for p in Preset::ALL {
                            ui.selectable_value(&mut ed.preset, p, p.label());
                        }
                    });
                }
                Kind::Shortcut => {
                    ui.horizontal(|ui| {
                        let (text, color) = if ed.recording {
                            ("Press the keys now...".to_string(), ACCENT)
                        } else {
                            match &ed.combo {
                                Some(c) => (c.to_string(), TEXT),
                                None => ("Nothing recorded".to_string(), MUTED),
                            }
                        };
                        egui::Frame::new().fill(Color32::from_rgb(18, 20, 24)).stroke(Stroke::new(1.0, if ed.recording { ACCENT } else { BORDER })).corner_radius(8).inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
                            ui.set_min_width(210.0);
                            ui.label(RichText::new(text).monospace().color(color));
                        });
                        if ui.button(if ed.recording { "Cancel" } else { "Record" }).clicked() {
                            ed.recording = !ed.recording;
                        }
                        ui.checkbox(&mut ed.win, "+ Win");
                    });
                    ui.label(muted("Windows reserves many Win+key combos, so tick \"+ Win\" instead of pressing the Win key."));
                }
                Kind::Mouse => {
                    egui::ComboBox::from_id_salt("mouse").selected_text(ed.mouse.label()).show_ui(ui, |ui| {
                        for b in Button::ALL {
                            ui.selectable_value(&mut ed.mouse, b, b.label());
                        }
                    });
                }
                Kind::Run => {
                    ui.add(egui::TextEdit::singleline(&mut ed.run).hint_text("C:\\path\\app.exe arguments").desired_width(f32::INFINITY));
                }
                Kind::Passthrough | Kind::Disabled => {}
            }
            ui.add_space(8.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(theme::primary("OK")).clicked() {
                    match ed.build() {
                        Ok(a) => apply = Some((ed.button, a)),
                        Err(e) => err = Some(e),
                    }
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
        if modal.should_close() && !ed.recording {
            close = true;
        }
        if let Some((b, a)) = apply {
            self.map_mut().set(b, a);
            self.dirty = true;
            self.status.clear();
            close = true;
        }
        if let Some(e) = err {
            self.note(e, true);
        }
        if close {
            self.editor = None;
        }
    }
}

// ---------- small widgets ----------

fn group_label(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.label(RichText::new(text).size(11.5).color(MUTED).strong());
    });
}

fn nav_row(ui: &mut egui::Ui, selected: bool, title: &str, subtitle: &str, dot: Option<Color32>, reserve_right: f32) -> (egui::Response, egui::Rect) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 9, ACCENT_DIM);
        p.rect_stroke(rect, 9, Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        p.rect_filled(rect, 9, CARD_HOVER);
    }
    let mut x = rect.left() + 12.0;
    if let Some(c) = dot {
        p.circle_filled(egui::pos2(x + 3.0, rect.center().y), 4.0, c);
        x += 16.0;
    }
    let max_w = rect.right() - x - reserve_right - 6.0;
    let clip = |s: &str, font: egui::FontId| -> String {
        let w = |t: &str| p.layout_no_wrap(t.to_owned(), font.clone(), TEXT).size().x;
        let mut s = s.to_owned();
        if w(&s) > max_w {
            while !s.is_empty() && w(&format!("{s}…")) > max_w {
                s.pop();
            }
            s.push('…');
        }
        s
    };
    p.text(egui::pos2(x, rect.top() + 9.0), egui::Align2::LEFT_TOP, clip(title, egui::FontId::proportional(14.5)), egui::FontId::proportional(14.5), TEXT);
    p.text(egui::pos2(x, rect.bottom() - 8.0), egui::Align2::LEFT_BOTTOM, clip(subtitle, egui::FontId::proportional(12.0)), egui::FontId::proportional(12.0), MUTED);
    (resp.on_hover_cursor(egui::CursorIcon::PointingHand), rect)
}

fn nav_item(ui: &mut egui::Ui, selected: bool, title: &str, subtitle: &str, dot: Option<Color32>) -> egui::Response {
    nav_row(ui, selected, title, subtitle, dot, 0.0).0
}

/// Like [`nav_item`] with a small cross on the right; the bool says the cross was clicked.
fn nav_item_removable(ui: &mut egui::Ui, selected: bool, title: &str, subtitle: &str) -> (egui::Response, bool) {
    let (resp, rect) = nav_row(ui, selected, title, subtitle, None, 26.0);
    let x_rect = egui::Rect::from_center_size(egui::pos2(rect.right() - 16.0, rect.center().y), vec2(22.0, 22.0));
    let hover = ui.rect_contains_pointer(x_rect);
    let clicked = hover && resp.clicked();
    if resp.hovered() || selected {
        let c = if hover { ERR } else { MUTED };
        let m = x_rect.center();
        let d = 4.0;
        let st = Stroke::new(1.6, c);
        ui.painter().line_segment([m + vec2(-d, -d), m + vec2(d, d)], st);
        ui.painter().line_segment([m + vec2(-d, d), m + vec2(d, -d)], st);
    }
    if hover {
        resp.clone().on_hover_text("Remove this profile");
    }
    (resp, clicked)
}

const POSITIONS: [&str; 9] = [
    "top-left",
    "top-center",
    "top-right",
    "center-left",
    "center-center",
    "center-right",
    "bottom-left",
    "bottom-center",
    "bottom-right",
];

fn position_label(p: &str) -> String {
    let mut parts = p.split('-');
    let (v, h) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let cap = |s: &str| s.chars().next().map(|c| c.to_uppercase().collect::<String>() + &s[1..]).unwrap_or_default();
    format!("{} {}", cap(v), h)
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
        None | Some(Action::Passthrough) => "Unchanged".into(),
        Some(Action::None) => "Blocked".into(),
        Some(Action::Preset(p)) => p.label().into(),
        Some(Action::Key(c)) => c.to_string(),
        Some(Action::Mouse(b)) => format!("Acts as {}", b.label()),
        Some(Action::Run(r)) => format!("Run {r}"),
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
                    } else if *key == egui::Key::S && modifiers.command && self.dirty {
                        self.save();
                    } else if *key == egui::Key::Escape && self.detecting {
                        self.detecting = false;
                    }
                }
                egui::Event::PointerButton { button, pressed: true, .. } if self.detecting => {
                    let b = pointer_button(*button);
                    self.detecting = false;
                    if matches!(self.sel, Sel::Global | Sel::Device(_) | Sel::App(_)) {
                        self.editor = Some(Editor::from(b, self.map().get(b)));
                    }
                }
                _ => {}
            }
        }

        // While detecting, ask the daemon to pass every button through (it
        // would otherwise swallow mapped ones). Renewed often; the daemon
        // also auto-resumes if we vanish.
        if self.detecting {
            if self.suspend_sent.is_none_or(|t| t.elapsed() > Duration::from_secs(2)) {
                nmm_platform_windows::set_remap_suspended(true);
                self.suspend_sent = Some(Instant::now());
            }
            ctx.request_repaint_after(Duration::from_secs(2));
        } else if self.suspend_sent.take().is_some() {
            nmm_platform_windows::set_remap_suspended(false);
        }

        // `NMM_GUI_SHOT=<file.png>`: save a screenshot of the window and exit (README screenshots).
        if let Ok(file) = std::env::var("NMM_GUI_SHOT") {
            self.frames += 1;
            ctx.request_repaint();
            if self.frames == 25 {
                ctx.request_screenshot(move |img| {
                    let raw: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                    let _ = image::save_buffer(&file, &raw, img.size[0] as u32, img.size[1] as u32, image::ColorType::Rgba8);
                    std::process::exit(0);
                });
            }
        }

        if self.mice_at.elapsed() >= MICE_REFRESH {
            self.refresh_mice();
        }
        ctx.request_repaint_after(MICE_REFRESH);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        nmm_platform_windows::set_remap_suspended(false);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        egui::Panel::left("nav")
            .exact_size(244.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(egui::Margin::symmetric(12, 10)).stroke(Stroke::new(1.0, BORDER)))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.sidebar(ui));
            });

        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(egui::Margin::symmetric(18, 10)).stroke(Stroke::new(1.0, BORDER)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let btn = theme::primary("Save");
                    if ui.add_enabled(self.dirty, btn).on_hover_text("Ctrl+S").clicked() {
                        self.save();
                    }
                    if self.dirty {
                        theme::pill(ui, "Unsaved changes", WARN);
                    } else if self.status.is_empty() {
                        ui.label(muted("All changes saved"));
                    }
                    if !self.status.is_empty() {
                        ui.label(RichText::new(&self.status).color(if self.status_err { ERR } else { OK }));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(muted(self.path.display().to_string()).small());
                    });
                });
                for w in self.cfg.warnings() {
                    ui.label(RichText::new(format!("Warning: {w}")).color(WARN));
                }
            });

        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin::symmetric(26, 20))).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mut clicked = None;
                match self.sel.clone() {
                    Sel::Global => clicked = self.page_global(ui),
                    Sel::Device(k) => clicked = self.page_device(ui, k),
                    Sel::App(i) if i < self.cfg.apps.len() => clicked = self.page_app(ui, i),
                    Sel::App(_) => self.sel = Sel::Global,
                    Sel::Settings => self.page_settings(ui),
                }
                if let Some(b) = clicked {
                    self.detecting = false;
                    self.editor = Some(Editor::from(b, self.map().get(b)));
                }
            });
        });

        self.editor_dialog(&ctx);
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

    #[test]
    fn position_labels() {
        assert_eq!(position_label("bottom-center"), "Bottom center");
        for p in POSITIONS {
            assert!(!position_label(p).is_empty());
        }
    }
}

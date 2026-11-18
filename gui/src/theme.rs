//! Look and feel: one dark theme, a few reusable widgets.

use eframe::egui::{self, Color32, CornerRadius, FontId, Margin, RichText, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(22, 24, 29);
pub const SIDEBAR: Color32 = Color32::from_rgb(27, 29, 36);
pub const CARD: Color32 = Color32::from_rgb(33, 36, 44);
pub const CARD_HOVER: Color32 = Color32::from_rgb(40, 44, 54);
pub const BORDER: Color32 = Color32::from_rgb(50, 54, 65);
pub const TEXT: Color32 = Color32::from_rgb(232, 234, 240);
pub const MUTED: Color32 = Color32::from_rgb(143, 149, 164);
pub const ACCENT: Color32 = Color32::from_rgb(91, 140, 255);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(38, 55, 98);
pub const OK: Color32 = Color32::from_rgb(76, 201, 138);
pub const WARN: Color32 = Color32::from_rgb(240, 184, 80);
pub const ERR: Color32 = Color32::from_rgb(244, 108, 108);

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(14);
    v.extreme_bg_color = Color32::from_rgb(18, 20, 24);
    v.faint_bg_color = CARD;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    let r = CornerRadius::same(8);
    let w = &mut v.widgets;
    for (s, fill) in [(&mut w.inactive, CARD_HOVER), (&mut w.hovered, Color32::from_rgb(52, 57, 70)), (&mut w.active, ACCENT_DIM)] {
        s.corner_radius = r;
        s.bg_fill = fill;
        s.weak_bg_fill = fill;
        s.bg_stroke = Stroke::new(1.0, BORDER);
        s.fg_stroke = Stroke::new(1.0, TEXT);
    }
    w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    w.noninteractive.corner_radius = r;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.open.corner_radius = r;
    ctx.set_visuals(v);

    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(10.0, 10.0);
        s.spacing.button_padding = egui::vec2(14.0, 7.0);
        s.spacing.interact_size.y = 30.0;
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(24.0));
        s.text_styles.insert(TextStyle::Body, FontId::proportional(15.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(15.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(12.5));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(14.0));
    });
}

/// A rounded panel with a border.
pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(12)
        .inner_margin(Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).color(MUTED).size(13.5)
}

pub fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).strong().size(17.0));
    if !subtitle.is_empty() {
        ui.label(muted(subtitle));
    }
}

/// Filled call-to-action button.
pub fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_owned()).strong().color(Color32::WHITE)).fill(ACCENT).stroke(Stroke::NONE).min_size(egui::vec2(110.0, 34.0))
}

/// Small coloured badge.
pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .corner_radius(20)
        .inner_margin(Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).size(12.5).strong());
        });
}

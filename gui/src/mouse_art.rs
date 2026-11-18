//! A drawn mouse with a clickable chip for every button.

use crate::theme::*;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, vec2};
use nmm_core::Button;

/// What to show for one button.
pub struct Slot {
    pub button: Button,
    pub text: String,
    /// Remapped (as opposed to unchanged).
    pub mapped: bool,
}

const CHIP_W: f32 = 230.0;
const CHIP_H: f32 = 54.0;

/// Draws the mouse; returns the button the user clicked, if any.
pub fn show(ui: &mut egui::Ui, slots: &[Slot], selected: Option<Button>) -> Option<Button> {
    let width = ui.available_width().min(640.0);
    let (outer, _) = ui.allocate_exact_size(vec2(ui.available_width(), 262.0), Sense::hover());
    let rect = Rect::from_center_size(outer.center(), vec2(width, outer.height()));
    let p = ui.painter_at(outer);
    let cx = rect.center().x;
    let body = Rect::from_min_size(Pos2::new(cx - 70.0, rect.top() + 14.0), vec2(140.0, 236.0));
    let mut clicked = None;

    let slot = |b: Button| slots.iter().find(|s| s.button == b);
    let fill = |ui: &egui::Ui, b: Button, resp: &egui::Response| -> Color32 {
        let mapped = slot(b).is_some_and(|s| s.mapped);
        let base = if mapped { ACCENT_DIM } else { Color32::from_rgb(54, 59, 72) };
        let _ = ui;
        if resp.hovered() { base.gamma_multiply(1.35) } else { base }
    };

    // Body.
    let body_r = CornerRadius { nw: 66, ne: 66, sw: 50, se: 50 };
    p.rect_filled(body.translate(vec2(0.0, 4.0)), body_r, Color32::from_black_alpha(70));
    p.rect_filled(body, body_r, Color32::from_rgb(42, 46, 57));
    p.rect_stroke(body, body_r, Stroke::new(1.5, BORDER), StrokeKind::Inside);

    // (button, area, corner radius)
    let top = body.top() + 6.0;
    let split_y = body.top() + 104.0;
    let areas: [(Button, Rect, CornerRadius); 5] = [
        (Button::Left, Rect::from_min_max(Pos2::new(body.left() + 6.0, top), Pos2::new(cx - 2.0, split_y)), CornerRadius { nw: 60, ne: 4, sw: 6, se: 6 }),
        (Button::Right, Rect::from_min_max(Pos2::new(cx + 2.0, top), Pos2::new(body.right() - 6.0, split_y)), CornerRadius { nw: 4, ne: 60, sw: 6, se: 6 }),
        (Button::Middle, Rect::from_center_size(Pos2::new(cx, top + 44.0), vec2(16.0, 40.0)), CornerRadius::same(8)),
        (Button::X2, Rect::from_min_size(Pos2::new(body.left() - 8.0, body.top() + 118.0), vec2(16.0, 34.0)), CornerRadius::same(6)),
        (Button::X1, Rect::from_min_size(Pos2::new(body.left() - 8.0, body.top() + 160.0), vec2(16.0, 34.0)), CornerRadius::same(6)),
    ];
    let mut centers = Vec::new();
    for (b, area, cr) in areas {
        let resp = ui.interact(area, ui.scope_id().with(("mouse-btn", b.name())), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        p.rect_filled(area, cr, fill(ui, b, &resp));
        let sel = selected == Some(b);
        p.rect_stroke(area, cr, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { ACCENT } else { BORDER }), StrokeKind::Inside);
        if resp.clicked() {
            clicked = Some(b);
        }
        centers.push((b, area.center()));
    }
    // Logo-less palm area hint.
    p.circle_filled(Pos2::new(cx, body.top() + 190.0), 3.0, BORDER);

    // Chips.
    let chip_pos = |b: Button| -> (Rect, bool) {
        let left_x = rect.left();
        let right_x = rect.right() - CHIP_W;
        let y = body.top();
        match b {
            Button::Left => (Rect::from_min_size(Pos2::new(left_x, y), vec2(CHIP_W, CHIP_H)), true),
            Button::X2 => (Rect::from_min_size(Pos2::new(left_x, y + 100.0), vec2(CHIP_W, CHIP_H)), true),
            Button::X1 => (Rect::from_min_size(Pos2::new(left_x, y + 166.0), vec2(CHIP_W, CHIP_H)), true),
            Button::Right => (Rect::from_min_size(Pos2::new(right_x, y), vec2(CHIP_W, CHIP_H)), false),
            Button::Middle => (Rect::from_min_size(Pos2::new(right_x, y + 66.0), vec2(CHIP_W, CHIP_H)), false),
        }
    };
    for (b, target) in centers {
        let (chip, on_left) = chip_pos(b);
        let from = if on_left { Pos2::new(chip.right(), chip.center().y) } else { Pos2::new(chip.left(), chip.center().y) };
        p.line_segment([from, target], Stroke::new(1.2, BORDER));
        p.circle_filled(target, 2.5, MUTED);
        let resp = ui.interact(chip, ui.scope_id().with(("chip", b.name())), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let s = slot(b);
        let mapped = s.is_some_and(|s| s.mapped);
        let sel = selected == Some(b);
        p.rect_filled(chip, 10, if resp.hovered() { CARD_HOVER } else { CARD });
        p.rect_stroke(chip, 10, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel || resp.hovered() { ACCENT } else { BORDER }), StrokeKind::Inside);
        let inner = chip.shrink2(vec2(12.0, 7.0));
        p.text(inner.left_top(), egui::Align2::LEFT_TOP, b.label(), egui::FontId::proportional(12.5), MUTED);
        let text = s.map(|s| s.text.as_str()).unwrap_or("Unchanged");
        let shown = fit(&p, text, inner.width());
        p.text(
            Pos2::new(inner.left(), inner.bottom()),
            egui::Align2::LEFT_BOTTOM,
            shown,
            egui::FontId::proportional(15.0),
            if mapped { ACCENT } else { TEXT },
        );
        if resp.clicked() {
            clicked = Some(b);
        }
    }
    clicked
}

/// Truncate with an ellipsis so text fits in `width`.
fn fit(p: &egui::Painter, text: &str, width: f32) -> String {
    let font = egui::FontId::proportional(15.0);
    let w = |s: &str| p.layout_no_wrap(s.to_owned(), font.clone(), TEXT).size().x;
    if w(text) <= width {
        return text.to_owned();
    }
    let mut s: String = text.to_owned();
    while !s.is_empty() && w(&format!("{s}…")) > width {
        s.pop();
    }
    format!("{s}…")
}


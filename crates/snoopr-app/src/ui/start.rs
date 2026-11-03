//! Start screen: one card per fixed drive plus "choose a folder".

use crate::i18n::{Lang, free_of};
use crate::icons::{Icon, paint_icon};
use crate::theme::{ACCENT, BAR_BG, BG, BORDER, TEXT, TEXT_MUTED};
use egui::{Align2, FontId, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use snoopr_core::volume::Volume;
use std::path::PathBuf;

const CARD: egui::Vec2 = vec2(260.0, 112.0);
const GAP: f32 = 16.0;

fn card(ui: &mut Ui) -> (egui::Response, egui::Painter, Rect) {
    let (rect, resp) = ui.allocate_exact_size(CARD, Sense::click());
    let painter = ui.painter_at(rect);
    let hot = resp.hovered();
    painter.rect(
        rect,
        10.0,
        if hot { BG } else { BAR_BG },
        Stroke::new(1.0, if hot { ACCENT } else { BORDER }),
        StrokeKind::Inside,
    );
    (
        resp.on_hover_cursor(egui::CursorIcon::PointingHand),
        painter,
        rect,
    )
}

fn header_icon(p: &egui::Painter, rect: Rect, icon: Icon) {
    let r = Rect::from_center_size(rect.left_top() + vec2(30.0, 30.0), vec2(20.0, 20.0));
    paint_icon(p, r, icon, ACCENT);
}

/// Draws the start screen; returns the folder or drive the user picked.
pub fn show(ui: &mut Ui, lang: Lang, volumes: &[Volume]) -> Option<PathBuf> {
    let s = lang.s();
    let mut picked = None;
    ui.add_space(48.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(s.start_title).size(22.0).strong());
    });
    ui.add_space(28.0);
    let avail = ui.available_width();
    let cols = (((avail + GAP) / (CARD.x + GAP)).floor() as usize).max(1);
    let n = volumes.len() + 1;
    let per_row = cols.min(n);
    let row_w = per_row as f32 * (CARD.x + GAP) - GAP;
    let pad = ((avail - row_w) / 2.0).max(0.0);
    for chunk_start in (0..n).step_by(per_row) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            ui.add_space(pad);
            for i in chunk_start..(chunk_start + per_row).min(n) {
                if let Some(v) = volumes.get(i) {
                    if volume_card(ui, lang, v).clicked() {
                        picked = Some(v.root.clone());
                    }
                } else {
                    let (resp, p, rect) = card(ui);
                    header_icon(&p, rect, Icon::Folder);
                    p.text(
                        rect.left_top() + vec2(52.0, 30.0),
                        Align2::LEFT_CENTER,
                        s.choose_folder,
                        FontId::proportional(15.0),
                        TEXT,
                    );
                    if resp.clicked() {
                        // Synchronous dialog: acceptable for a one-off user choice.
                        picked = rfd::FileDialog::new().pick_folder();
                    }
                }
            }
        });
        ui.add_space(GAP);
    }
    picked
}

fn volume_card(ui: &mut Ui, lang: Lang, v: &Volume) -> egui::Response {
    let (resp, p, rect) = card(ui);
    header_icon(&p, rect, Icon::Drive);
    let letter: String = v.root.to_string_lossy().chars().take(2).collect();
    let at = rect.left_top() + vec2(52.0, 30.0);
    let font = FontId::proportional(17.0);
    // Faux bold: draw the letter twice, offset by half a pixel.
    for dx in [0.0, 0.6] {
        p.text(
            at + vec2(dx, 0.0),
            Align2::LEFT_CENTER,
            &letter,
            font.clone(),
            TEXT,
        );
    }
    if !v.label.is_empty() {
        p.text(
            rect.left_top() + vec2(88.0, 31.0),
            Align2::LEFT_CENTER,
            &v.label,
            FontId::proportional(13.0),
            TEXT_MUTED,
        );
    }
    let bar = Rect::from_min_size(rect.left_top() + vec2(16.0, 60.0), vec2(CARD.x - 32.0, 6.0));
    p.rect_filled(bar, 3.0, BORDER);
    let used = v.total.saturating_sub(v.free);
    let frac = if v.total == 0 {
        0.0
    } else {
        (used as f32 / v.total as f32).clamp(0.0, 1.0)
    };
    let mut fill = bar;
    fill.set_width(bar.width() * frac);
    p.rect_filled(fill, 3.0, ACCENT);
    p.text(
        rect.left_top() + vec2(16.0, 84.0),
        Align2::LEFT_CENTER,
        free_of(lang, v.free, v.total),
        FontId::proportional(12.5),
        TEXT_MUTED,
    );
    resp
}

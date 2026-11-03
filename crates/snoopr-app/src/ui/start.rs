//! Start screen: one card per fixed drive plus "choose a folder".

use crate::anim::{ease_out_cubic, motion_enabled};
use crate::i18n::{Lang, free_of};
use crate::icons::{Icon, paint_icon};
use crate::theme::{ACCENT, BAR_BG, BG, BORDER, TEXT, TEXT_MUTED};
use egui::{Align2, FontId, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use snoopr_core::volume::Volume;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const CARD: egui::Vec2 = vec2(260.0, 112.0);
const GAP: f32 = 16.0;
/// Time a usage bar takes to fill, and the delay between two cards.
const FILL: Duration = Duration::from_millis(600);
const STAGGER: Duration = Duration::from_millis(60);

/// Share of `frac` shown `elapsed` after the screen appeared: card `index`
/// starts `index` staggers late and fills with an ease-out curve.
pub fn bar_fraction(frac: f32, index: usize, elapsed: Duration, enabled: bool) -> f32 {
    if !enabled {
        return frac;
    }
    let delay = STAGGER * u32::try_from(index).unwrap_or(u32::MAX);
    let t = elapsed.saturating_sub(delay).as_secs_f64() / FILL.as_secs_f64();
    frac * ease_out_cubic(t.min(1.0) as f32)
}

fn card(ui: &mut Ui) -> (egui::Response, egui::Painter, Rect) {
    let (rect, resp) = ui.allocate_exact_size(CARD, Sense::click());
    let painter = ui.painter_at(rect);
    // Eased hover state (0 -> 1); instant when animations are off.
    let time = if motion_enabled() { 0.12 } else { 0.0 };
    let hot = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), time);
    painter.rect(
        rect,
        10.0,
        BAR_BG.lerp_to_gamma(BG, hot),
        Stroke::new(1.0, BORDER.lerp_to_gamma(ACCENT, hot)),
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
///
/// `appeared` is when the screen first showed: the usage bars fill from there.
pub fn show(ui: &mut Ui, lang: Lang, volumes: &[Volume], appeared: Instant) -> Option<PathBuf> {
    let s = lang.s();
    let mut picked = None;
    let elapsed = appeared.elapsed();
    let enabled = motion_enabled();
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
                    if volume_card(ui, lang, v, i, elapsed).clicked() {
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
    // The bars of the last card are done once its delay and fill have passed.
    if enabled && elapsed < FILL + STAGGER * u32::try_from(n).unwrap_or(u32::MAX) {
        ui.ctx().request_repaint();
    }
    picked
}

fn volume_card(
    ui: &mut Ui,
    lang: Lang,
    v: &Volume,
    index: usize,
    elapsed: Duration,
) -> egui::Response {
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
    fill.set_width(bar.width() * bar_fraction(frac, index, elapsed, motion_enabled()));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn bars_fill_in_turn() {
        assert_eq!(bar_fraction(0.8, 0, ms(0), true), 0.0);
        assert_eq!(bar_fraction(0.8, 0, ms(600), true), 0.8);
        assert_eq!(bar_fraction(0.8, 1, ms(60), true), 0.0);
        assert_eq!(bar_fraction(0.8, 1, ms(660), true), 0.8);
        assert_eq!(bar_fraction(0.8, 3, ms(0), false), 0.8);
    }
}

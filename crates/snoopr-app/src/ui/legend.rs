//! The colour legend above the status bar (spec §6.3): one chip per content
//! type, then the hatched "not scanned" chip.

use crate::i18n::{Lang, type_name};
use crate::theme::{HATCH, TEXT_MUTED, UNSCANNED_FILL, type_color};
use crate::treemap::paint::hatch;
use egui::{Color32, RichText, Sense, Ui, vec2};
use snoopr_core::filetype::FileType;

const CHIP: f32 = 10.0;
const CHIP_RADIUS: f32 = 3.0;
const FONT: f32 = 11.0;
/// Between a chip and its text.
const CHIP_GAP: f32 = 5.0;
/// Between two entries.
const ENTRY_GAP: f32 = 14.0;

fn entry(ui: &mut Ui, fill: Color32, hatched: bool, label: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(CHIP, CHIP), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, CHIP_RADIUS, fill);
    if hatched {
        hatch(p, r, HATCH, 4.0);
    }
    ui.label(RichText::new(label).size(FONT).color(TEXT_MUTED));
}

/// Draws the legend on one row, vertically centred in the panel.
pub fn show(ui: &mut Ui, lang: Lang) {
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = CHIP_GAP;
        for t in FileType::ALL {
            entry(ui, type_color(t), false, type_name(lang, t));
            ui.add_space(ENTRY_GAP - CHIP_GAP);
        }
        entry(ui, UNSCANNED_FILL, true, lang.s().unscanned_name);
    });
}

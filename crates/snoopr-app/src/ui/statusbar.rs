//! The status bar (spec §6.4): selection or scan progress, freed space, Recycle Bin, admin relaunch.

use super::node_label;
use crate::i18n::{
    Lang, bin_still, block_message, deleting, denied_dirs, format_size, freed, modified_ago,
    scan_done, scanning, used_of, warning_message,
};
use crate::icons::{Icon, paint_icon};
use crate::session::Session;
use crate::theme::{LOCK_TEXT, OK_TEXT, TEXT, TEXT_MUTED, WARN_TEXT};
use egui::{Align, Color32, FontId, Label, Layout, RichText, Sense, Ui, UiBuilder, vec2};
use snoopr_core::actions::verdict;
use snoopr_core::age::{age_months, now_unix};
use snoopr_core::rules::{RuleContext, Verdict};
use snoopr_core::tree::{NodeKind, Tree};
use std::sync::PoisonError;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusAction {
    None,
    EmptyBin,
    RelaunchAdmin,
}

const FONT: f32 = 11.5;
const GAP: f32 = 12.0;

fn text(t: impl Into<String>, color: Color32) -> RichText {
    RichText::new(t).size(FONT).color(color)
}

/// A small button with the dark-red look of the mockup; true when clicked.
fn red_button(ui: &mut Ui, label: &str, enabled: bool) -> bool {
    let b = egui::Button::new(text(label, LOCK_TEXT))
        .fill(Color32::from_rgb(0x2a, 0x1d, 0x1d))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0x4a, 0x2a, 0x2a)))
        .corner_radius(6.0)
        .min_size(vec2(0.0, 20.0));
    let resp = ui.add_enabled(enabled, b);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    } else {
        false
    }
}

pub fn show(
    ui: &mut Ui,
    s: &Session,
    rules: &RuleContext,
    lang: Lang,
    elevated: bool,
) -> StatusAction {
    let st = lang.s();
    let full = ui.max_rect();
    let mut action = StatusAction::None;

    let right = ui.scope_builder(
        UiBuilder::new()
            .max_rect(full)
            .layout(Layout::right_to_left(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            if s.bin_bytes > 0 {
                if red_button(ui, st.empty_bin, !s.is_busy()) {
                    action = StatusAction::EmptyBin;
                }
                ui.label(text(bin_still(lang, s.bin_bytes), TEXT_MUTED));
            }
            if s.freed > 0 {
                ui.label(text(freed(lang, s.freed), OK_TEXT));
            }
        },
    );
    let mut left_rect = full;
    if right.response.rect.width() > 0.0 {
        left_rect.max.x = right.response.rect.left() - GAP;
    }

    // The "inaccessible folders" notice hugs the right end of the left area so
    // the long selection text is what gets truncated.
    if s.denied > 0 {
        let d = ui.scope_builder(
            UiBuilder::new()
                .max_rect(left_rect)
                .layout(Layout::right_to_left(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if !elevated && red_button(ui, st.relaunch_admin, !s.is_deleting()) {
                    action = StatusAction::RelaunchAdmin;
                }
                ui.label(text(denied_dirs(lang, s.denied), WARN_TEXT));
            },
        );
        left_rect.max.x = d.response.rect.left() - GAP;
    }

    ui.scope_builder(
        UiBuilder::new()
            .max_rect(left_rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.set_clip_rect(left_rect.intersect(ui.clip_rect()));
            ui.spacing_mut().item_spacing.x = GAP;
            left_text(ui, s, rules, lang);
        },
    );
    action
}

fn left_text(ui: &mut Ui, s: &Session, rules: &RuleContext, lang: Lang) {
    if let Some(job) = &s.delete {
        let done = job.progress.done.load(Ordering::Relaxed);
        let total = job.progress.total.load(Ordering::Relaxed);
        ui.add(Label::new(text(deleting(lang, done, total), TEXT)).extend());
        return;
    }
    if s.empty_job.is_some() {
        ui.add(Label::new(text(lang.s().emptying_bin, TEXT)).extend());
        return;
    }
    if let Some(scan) = &s.scan {
        let files = scan.progress.files.load(Ordering::Relaxed);
        ui.add(Label::new(text(scanning(lang, files, scan.elapsed().as_secs()), TEXT)).extend());
        let cur = scan
            .progress
            .current
            .lock()
            .map(|c| c.display().to_string())
            .unwrap_or_default();
        ui.add(Label::new(text(cur, TEXT_MUTED)).truncate());
        return;
    }
    let tree = s.tree.read().unwrap_or_else(PoisonError::into_inner);
    let sel = s.view.selected;
    if sel == tree.root() {
        let mut line = node_label(&tree, sel, lang);
        let (used, total) = (s.usage.0.saturating_sub(s.usage.1), s.usage.0);
        line.push_str(" · ");
        if total > 0 {
            line.push_str(&used_of(lang, used, total));
        } else {
            line.push_str(&format_size(lang, tree.node(sel).size));
        }
        ui.add(Label::new(text(line, TEXT)).extend());
        if let Some(t) = s.scan_time {
            let done = scan_done(lang, t.as_secs_f64(), s.scan_files);
            ui.add(Label::new(text(done, TEXT_MUTED)).truncate());
        }
        return;
    }
    selection_text(ui, &tree, s, rules, lang);
}

/// `name · size · modified · path`, then the guard-rail message in color.
fn selection_text(ui: &mut Ui, tree: &Tree, s: &Session, rules: &RuleContext, lang: Lang) {
    let sel = s.view.selected;
    let node = tree.node(sel);
    let mut head = format!(
        "{} · {}",
        node_label(tree, sel, lang),
        format_size(lang, node.size)
    );
    if node.kind == NodeKind::Unscanned {
        ui.add(Label::new(text(head, TEXT)).extend());
        return;
    }
    head.push_str(" · ");
    head.push_str(&modified_ago(lang, age_months(now_unix(), node.newest)));
    ui.add(Label::new(text(head, TEXT)).extend());
    let path = tree.path(sel).display().to_string();
    let verdict = verdict(tree, rules, sel);
    let mono = RichText::new(path)
        .font(FontId::monospace(FONT))
        .color(TEXT_MUTED.gamma_multiply(0.8));
    if verdict == Verdict::Allow {
        ui.add(Label::new(mono).truncate());
        return;
    }
    ui.add(Label::new(mono).extend());
    let (icon, color, msg) = match verdict {
        Verdict::Block(b) => (Icon::Lock, LOCK_TEXT, block_message(lang, b)),
        Verdict::Warn(w) => (Icon::Warn, WARN_TEXT, warning_message(lang, w)),
        Verdict::Allow => return,
    };
    ui.spacing_mut().item_spacing.x = 5.0;
    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
    paint_icon(ui.painter(), r, icon, color);
    ui.add(Label::new(text(msg, color)).truncate());
}

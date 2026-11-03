//! The deletion dialogs (spec §7): confirmation, "too big for the Recycle
//! Bin", emptying the Recycle Bin, and the failures recap.
//!
//! Nothing is deleted from here: a confirm button returns a [`ModalAction`]
//! and [`next_step`] maps it to a plan, which `app.rs` hands to the session.

use crate::i18n::{
    Lang, confirm_one_title, confirm_title, empty_bin_body, failure_reason, format_size,
    warning_message,
};
use crate::icons::{Icon, paint_icon};
use crate::session::{Session, Subject};
use crate::system;
use crate::theme::{ACCENT, DANGER, LOCK_TEXT, POPUP_BG, TEXT, TEXT_MUTED, WARN_TEXT, bold};
use crate::ui::node_label;
use egui::{
    Align, Color32, CursorIcon, FontId, Id, Layout, Margin, Painter, Rect, RichText, Sense, Stroke,
    Ui, pos2, vec2,
};
use snoopr_core::actions::verdict;
use snoopr_core::delete::recycle::{RecycleCapacity, fits_in_recycle_bin};
use snoopr_core::delete::{DeleteMode, Failure};
use snoopr_core::rules::{RuleContext, Verdict};
use snoopr_core::tree::NodeId;
use std::sync::{OnceLock, PoisonError};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum Modal {
    ConfirmDelete(Subject),
    TooBig {
        subject: Subject,
        fit: Vec<NodeId>,
        too_big: Vec<NodeId>,
    },
    ConfirmEmptyBin,
    Failures(Vec<Failure>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalAction {
    None,
    Close,
    Recycle,
    DeletePermanently,
    TooBigDeleteAll,
    TooBigSkip,
    EmptyBin,
}

/// Deletion requests as `(mode, ids)`, run in this order.
pub type Plan = Vec<(DeleteMode, Vec<NodeId>)>;

/// What a click in a dialog leads to.
#[derive(Debug, PartialEq)]
pub enum Step {
    /// Nothing to do.
    Stay,
    Close,
    /// Replace the dialog by this one.
    Open(Modal),
    Delete(Subject, Plan),
    EmptyBin,
}

/// Splits `(id, size)` items into those the Recycle Bin can take and the others.
pub fn split_for_recycle(
    items: &[(NodeId, u64)],
    cap: RecycleCapacity,
) -> (Vec<NodeId>, Vec<NodeId>) {
    let mut fit = Vec::new();
    let mut too_big = Vec::new();
    for &(id, size) in items {
        if fits_in_recycle_bin(size, cap) {
            fit.push(id);
        } else {
            too_big.push(id);
        }
    }
    (fit, too_big)
}

/// Drops empty requests; with nothing left the dialog just closes.
fn plan(subject: Subject, p: Plan) -> Step {
    let p: Plan = p.into_iter().filter(|(_, ids)| !ids.is_empty()).collect();
    if p.is_empty() {
        Step::Close
    } else {
        Step::Delete(subject, p)
    }
}

/// Maps a click in dialog `m` to the next step. `items` are the deletable
/// items of the dialog's subject `(id, size)`; `cap` is read only for the
/// Recycle Bin choice.
/// Only the confirm buttons of the matching dialog delete anything, and the
/// Recycle Bin choice never plans a permanent deletion: items that do not fit
/// open the "too big" question first (spec §7.2).
pub fn next_step(
    m: &Modal,
    a: ModalAction,
    items: &[(NodeId, u64)],
    cap: impl FnOnce() -> RecycleCapacity,
) -> Step {
    use DeleteMode::{Permanent, RecycleBin};
    use ModalAction as A;
    match (m, a) {
        (_, A::None) => Step::Stay,
        (_, A::Close) => Step::Close,
        (Modal::ConfirmDelete(subject), A::Recycle) => {
            let (fit, too_big) = split_for_recycle(items, cap());
            if too_big.is_empty() {
                plan(*subject, vec![(RecycleBin, fit)])
            } else {
                Step::Open(Modal::TooBig {
                    subject: *subject,
                    fit,
                    too_big,
                })
            }
        }
        (Modal::ConfirmDelete(subject), A::DeletePermanently) => plan(
            *subject,
            vec![(Permanent, items.iter().map(|&(id, _)| id).collect())],
        ),
        (
            Modal::TooBig {
                subject,
                fit,
                too_big,
            },
            A::TooBigDeleteAll,
        ) => plan(
            *subject,
            vec![(RecycleBin, fit.clone()), (Permanent, too_big.clone())],
        ),
        (Modal::TooBig { subject, fit, .. }, A::TooBigSkip) => {
            plan(*subject, vec![(RecycleBin, fit.clone())])
        }
        (Modal::ConfirmEmptyBin, A::EmptyBin) => Step::EmptyBin,
        // A button of another dialog: ignored.
        _ => Step::Stay,
    }
}

/// Shortest time a new dialog ignores its buttons.
const MIN_ARM_DELAY: Duration = Duration::from_millis(300);
/// Double-click time assumed where the system one is unknown.
const FALLBACK_DOUBLE_CLICK: Duration = Duration::from_millis(500);

/// How long a new dialog ignores its buttons: at least the double-click time,
/// so the second press of a double-click (it can come that late after the
/// first) never acts on a dialog that the first click opened.
pub fn arm_delay_for(double_click: Option<Duration>) -> Duration {
    double_click
        .unwrap_or(FALLBACK_DOUBLE_CLICK)
        .max(MIN_ARM_DELAY)
}

/// [`arm_delay_for`] of the system double-click time, read once.
pub fn arm_delay() -> Duration {
    static DELAY: OnceLock<Duration> = OnceLock::new();
    *DELAY.get_or_init(|| arm_delay_for(system::double_click_time()))
}

/// The buttons of a dialog act only once `delay` has passed since it appeared
/// **and** a pointer press started at or after that moment: a press that
/// started earlier (such as the second press of a double-click) never arms
/// them, however long it is held.
pub fn buttons_armed(
    opened_at: Instant,
    now: Instant,
    press: Option<Instant>,
    delay: Duration,
) -> bool {
    let Some(arms_at) = opened_at.checked_add(delay) else {
        return false;
    };
    now >= arms_at && press.is_some_and(|p| p >= arms_at)
}

/// The first pointer press that started once a dialog's buttons may arm;
/// earlier presses are not recorded.
#[derive(Clone, Copy, Debug)]
pub struct PressTracker {
    arms_at: Option<Instant>,
    first: Option<Instant>,
}

impl PressTracker {
    pub fn new(opened_at: Instant, delay: Duration) -> Self {
        PressTracker {
            arms_at: opened_at.checked_add(delay),
            first: None,
        }
    }

    /// Records the input of a pass run at `at`; `pressed_now` = a button went
    /// down in it.
    pub fn track(&mut self, at: Instant, pressed_now: bool) {
        if pressed_now && self.first.is_none() && self.arms_at.is_some_and(|a| at >= a) {
            self.first = Some(at);
        }
    }

    pub fn first_press(&self) -> Option<Instant> {
        self.first
    }
}

/// Keeps a button's action when the buttons are armed; before that, only
/// cancelling gets through (it is always safe).
fn gate(a: ModalAction, armed: bool) -> ModalAction {
    match a {
        ModalAction::None | ModalAction::Close => a,
        _ if armed => a,
        _ => ModalAction::None,
    }
}

/// An open dialog and its guard against clicks meant for something else.
/// Make a new one for every dialog shown, so the guard starts again.
pub struct OpenModal {
    pub modal: Modal,
    opened_at: Instant,
    delay: Duration,
    presses: PressTracker,
}

impl OpenModal {
    pub fn new(modal: Modal) -> Self {
        let opened_at = Instant::now();
        let delay = arm_delay();
        OpenModal {
            modal,
            opened_at,
            delay,
            presses: PressTracker::new(opened_at, delay),
        }
    }
}

const WIDTH: f32 = 480.0;
const ROW_H: f32 = 20.0;
const BACKDROP: Color32 = Color32::from_rgba_unmultiplied_const(5, 6, 9, 184);
const MODAL_BORDER: Color32 = Color32::from_rgb(0x2a, 0x2e, 0x39);
const LIST_BG: Color32 = Color32::from_rgb(0x12, 0x14, 0x1a);
const GHOST_BG: Color32 = Color32::from_rgb(0x1c, 0x1f, 0x27);
const INFO_BG: Color32 = Color32::from_rgb(0x1a, 0x22, 0x33);
const INFO_BORDER: Color32 = Color32::from_rgb(0x2b, 0x3a, 0x57);
const INFO_TEXT: Color32 = Color32::from_rgb(0xb9, 0xcd, 0xf5);

fn modal_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(POPUP_BG)
        .stroke(Stroke::new(1.0, MODAL_BORDER))
        .corner_radius(12)
        .inner_margin(20)
        .shadow(egui::Shadow {
            offset: [0, 20],
            blur: 60,
            spread: 0,
            color: Color32::from_black_alpha(128),
        })
}

/// Draws the dialog over a dimmed backdrop that blocks the rest of the window,
/// and returns the button clicked. A click on the backdrop closes it. Until
/// the buttons are armed (see [`buttons_armed`]) only Cancel acts.
pub fn show(
    ctx: &egui::Context,
    open: &mut OpenModal,
    s: &Session,
    rules: &RuleContext,
    lang: Lang,
) -> ModalAction {
    // A press is dated by the pass that sees it, right after it happened.
    let now = Instant::now();
    open.presses
        .track(now, ctx.input(|i| i.pointer.any_pressed()));
    let armed = buttons_armed(open.opened_at, now, open.presses.first_press(), open.delay);
    let m = &open.modal;
    let kind = match m {
        Modal::ConfirmDelete(_) => "confirm-delete",
        Modal::TooBig { .. } => "too-big",
        Modal::ConfirmEmptyBin => "confirm-empty-bin",
        Modal::Failures(_) => "failures",
    };
    let r = egui::Modal::new(Id::unique(("snoopr-modal", kind)))
        .backdrop_color(BACKDROP)
        .frame(modal_frame())
        .show(ctx, |ui| {
            ui.set_width(WIDTH);
            ui.spacing_mut().item_spacing.y = 6.0;
            match m {
                Modal::ConfirmDelete(subject) => confirm_delete(ui, s, *subject, rules, lang),
                Modal::TooBig {
                    subject, too_big, ..
                } => too_big_dialog(ui, s, *subject, too_big, lang),
                Modal::ConfirmEmptyBin => confirm_empty_bin(ui, s, lang),
                Modal::Failures(f) => failures_dialog(ui, f, lang),
            }
        });
    if r.backdrop_response.clicked() {
        ModalAction::Close
    } else {
        gate(r.inner, armed)
    }
}

fn confirm_delete(
    ui: &mut Ui,
    s: &Session,
    subject: Subject,
    rules: &RuleContext,
    lang: Lang,
) -> ModalAction {
    let st = lang.s();
    // Exactly what a confirmation would send: the checked items of the subject.
    let items = s.delete_items(subject, &s.subject_ids(subject));
    let total: u64 = items.iter().map(|it| it.size).sum();
    let heading = match (subject, items.as_slice()) {
        (Subject::One(_), [only]) => {
            let tree = s.tree.read().unwrap_or_else(PoisonError::into_inner);
            confirm_one_title(lang, &node_label(&tree, only.id, lang))
        }
        _ => confirm_title(lang, items.len()),
    };
    title(ui, &heading);
    ui.label(
        RichText::new(format_size(lang, total))
            .font(bold(13.0))
            .color(TEXT),
    );
    let rows: Vec<(String, String, Option<&str>)> = {
        let tree = s.tree.read().unwrap_or_else(PoisonError::into_inner);
        items
            .iter()
            .map(|it| {
                let warn = match verdict(&tree, rules, it.id) {
                    Verdict::Warn(w) => Some(warning_message(lang, w)),
                    _ => None,
                };
                (
                    it.path.display().to_string(),
                    format_size(lang, it.size),
                    warn,
                )
            })
            .collect()
    };
    list_box(ui, "confirm-list", |ui| {
        for (path, size, warn) in &rows {
            let note = warn.map(|text| Note {
                icon: Some(Icon::Warn),
                color: WARN_TEXT,
                text,
            });
            path_row(ui, path, size, note);
        }
    });
    info_box(ui, st.bin_info);
    buttons(ui, |ui| {
        let mut a = ModalAction::None;
        if button(ui, st.move_to_bin, Look::Primary) {
            a = ModalAction::Recycle;
        }
        if button(ui, st.delete_permanently, Look::DangerText) {
            a = ModalAction::DeletePermanently;
        }
        if button(ui, st.cancel, Look::Ghost) {
            a = ModalAction::Close;
        }
        a
    })
}

fn too_big_dialog(
    ui: &mut Ui,
    s: &Session,
    subject: Subject,
    too_big: &[NodeId],
    lang: Lang,
) -> ModalAction {
    let st = lang.s();
    title(ui, st.too_big_title);
    body(ui, st.too_big_body);
    let items = s.delete_items(subject, too_big);
    list_box(ui, "too-big-list", |ui| {
        for it in &items {
            path_row(
                ui,
                &it.path.display().to_string(),
                &format_size(lang, it.size),
                None,
            );
        }
    });
    buttons(ui, |ui| {
        let mut a = ModalAction::None;
        if button(ui, st.skip, Look::Primary) {
            a = ModalAction::TooBigSkip;
        }
        if button(ui, st.delete_permanently, Look::DangerText) {
            a = ModalAction::TooBigDeleteAll;
        }
        if button(ui, st.cancel, Look::Ghost) {
            a = ModalAction::Close;
        }
        a
    })
}

fn confirm_empty_bin(ui: &mut Ui, s: &Session, lang: Lang) -> ModalAction {
    let st = lang.s();
    let root = s.volume_root.display().to_string();
    let drive = root.trim_end_matches('\\');
    title(ui, st.empty_bin_title);
    body(ui, &empty_bin_body(lang, s.bin_bytes, drive));
    buttons(ui, |ui| {
        let mut a = ModalAction::None;
        if button(ui, st.empty_bin, Look::DangerFill) {
            a = ModalAction::EmptyBin;
        }
        if button(ui, st.cancel, Look::Ghost) {
            a = ModalAction::Close;
        }
        a
    })
}

fn failures_dialog(ui: &mut Ui, failures: &[Failure], lang: Lang) -> ModalAction {
    let st = lang.s();
    title(ui, st.failures_title);
    // One fixed-height row per failure, so a long list stays cheap.
    let row_h = 2.0 * ROW_H;
    list_frame().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical()
            .id_salt("failures-list")
            .max_height(260.0)
            .auto_shrink([false, true])
            .show_rows(ui, row_h, failures.len(), |ui, range| {
                for f in &failures[range] {
                    failure_row(ui, f, lang, row_h);
                }
            });
    });
    buttons(ui, |ui| {
        if button(ui, st.ok, Look::Primary) {
            ModalAction::Close
        } else {
            ModalAction::None
        }
    })
}

fn title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(bold(15.0)).color(TEXT));
}

fn body(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(TEXT_MUTED));
}

fn list_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(LIST_BG)
        .corner_radius(8)
        .inner_margin(Margin::symmetric(10, 6))
}

fn list_box(ui: &mut Ui, salt: &str, rows: impl FnOnce(&mut Ui)) {
    list_frame().show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        egui::ScrollArea::vertical()
            .id_salt(salt)
            .max_height(220.0)
            .auto_shrink([false, true])
            .show(ui, rows);
    });
}

/// The blue explanation box of the confirmation (spec §7.1).
fn info_box(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    egui::Frame::NONE
        .fill(INFO_BG)
        .stroke(Stroke::new(1.0, INFO_BORDER))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(10, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).size(11.0).color(INFO_TEXT));
        });
}

#[derive(Clone, Copy)]
enum Look {
    Ghost,
    /// Ghost with red text: "Delete permanently".
    DangerText,
    Primary,
    /// Red fill: "Empty Recycle Bin".
    DangerFill,
}

/// A dialog button; true when clicked.
fn button(ui: &mut Ui, label: &str, look: Look) -> bool {
    let ghost = Stroke::new(1.0, MODAL_BORDER);
    let (fill, stroke, color, font) = match look {
        Look::Ghost => (GHOST_BG, ghost, TEXT, FontId::proportional(12.5)),
        Look::DangerText => (GHOST_BG, ghost, LOCK_TEXT, FontId::proportional(12.5)),
        Look::Primary => (ACCENT, Stroke::NONE, Color32::WHITE, bold(12.5)),
        Look::DangerFill => (DANGER, Stroke::NONE, Color32::WHITE, bold(12.5)),
    };
    let b = egui::Button::new(RichText::new(label).font(font).color(color))
        .fill(fill)
        .stroke(stroke)
        .corner_radius(8)
        .min_size(vec2(0.0, 30.0));
    ui.add(b)
        .on_hover_cursor(CursorIcon::PointingHand)
        .clicked()
}

/// The button row, right-aligned: the first button added is the rightmost.
fn buttons(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> ModalAction) -> ModalAction {
    ui.add_space(10.0);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.spacing_mut().button_padding = vec2(12.0, 6.0);
        add(ui)
    })
    .inner
}

/// A line under a row: guard-rail warning or failure reason.
struct Note<'a> {
    icon: Option<Icon>,
    color: Color32,
    text: &'a str,
}

/// `text` cut at the start ("…" then its end) to fit in `max_w`: the end of a
/// path names the item.
fn elide_start(p: &Painter, text: &str, font: &FontId, max_w: f32) -> String {
    let width = |s: String| p.layout_no_wrap(s, font.clone(), TEXT).size().x;
    if width(text.to_owned()) <= max_w {
        return text.to_owned();
    }
    let starts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    // Smallest start whose tail fits; tails shrink as the start grows.
    let (mut lo, mut hi) = (0, starts.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        if width(format!("…{}", &text[starts[mid]..])) <= max_w {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    match starts.get(lo) {
        Some(&i) => format!("…{}", &text[i..]),
        None => "…".to_owned(),
    }
}

/// `path` on the left (elided at the start), `right` on the right, then an
/// optional wrapped note.
fn path_row(ui: &mut Ui, path: &str, right: &str, note: Option<Note>) {
    let p = ui.painter().clone();
    let w = ui.available_width();
    let note = note.map(|n| {
        let indent = if n.icon.is_some() { 18.0 } else { 0.0 };
        let g = p.layout(
            n.text.to_owned(),
            FontId::proportional(11.0),
            n.color,
            (w - indent).max(1.0),
        );
        (g, indent, n)
    });
    let h = ROW_H + note.as_ref().map_or(0.0, |(g, ..)| g.size().y + 3.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let cy = rect.top() + ROW_H / 2.0;
    let font = FontId::proportional(11.5);
    let rg = p.layout_no_wrap(right.to_owned(), font.clone(), TEXT_MUTED);
    let rx = rect.right() - rg.size().x;
    p.galley(pos2(rx, cy - rg.size().y / 2.0), rg, TEXT_MUTED);
    let shown = elide_start(&p, path, &font, (rx - rect.left() - 12.0).max(0.0));
    let elided = shown.len() != path.len();
    let pg = p.layout_no_wrap(shown, font, TEXT);
    p.galley(pos2(rect.left(), cy - pg.size().y / 2.0), pg, TEXT);
    if let Some((g, indent, n)) = note {
        let top = rect.top() + ROW_H;
        if let Some(icon) = n.icon {
            let ir = Rect::from_min_size(pos2(rect.left(), top), vec2(13.0, 13.0));
            paint_icon(&p, ir, icon, n.color);
        }
        p.galley(pos2(rect.left() + indent, top), g, n.color);
    }
    if elided {
        resp.on_hover_text(path);
    }
}

/// Path on the first line, reason on the second; both clipped to the row,
/// the full text on hover.
fn failure_row(ui: &mut Ui, f: &Failure, lang: Lang, h: f32) {
    let path = f.path.display().to_string();
    let reason = failure_reason(lang, &f.reason);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let font = FontId::proportional(11.5);
    let shown = elide_start(&p, &path, &font, rect.width());
    let pg = p.layout_no_wrap(shown, font, TEXT);
    let cy = rect.top() + ROW_H / 2.0;
    p.galley(pos2(rect.left(), cy - pg.size().y / 2.0), pg, TEXT);
    let rg = p.layout_no_wrap(reason.clone(), FontId::proportional(11.0), LOCK_TEXT);
    let cy = rect.top() + ROW_H * 1.5 - 2.0;
    p.galley(pos2(rect.left(), cy - rg.size().y / 2.0), rg, LOCK_TEXT);
    resp.on_hover_text(format!("{path} — {reason}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use snoopr_core::delete::recycle::RecycleCapacity;
    use snoopr_core::delete::{DeleteMode, Failure, FailureReason};
    use snoopr_core::tree::NodeId;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    const A: NodeId = NodeId(1);
    const B: NodeId = NodeId(2);
    const ITEMS: [(NodeId, u64); 2] = [(A, 50), (B, 150)];
    use DeleteMode::{Permanent, RecycleBin};

    #[test]
    fn split_for_recycle_by_capacity() {
        let split = |cap| split_for_recycle(&ITEMS, cap);
        assert_eq!(split(RecycleCapacity::MaxBytes(100)), (vec![A], vec![B]));
        assert_eq!(split(RecycleCapacity::Disabled), (vec![], vec![A, B]));
        assert_eq!(split(RecycleCapacity::Unknown), (vec![A, B], vec![]));
    }

    fn step(m: &Modal, a: ModalAction, cap: RecycleCapacity) -> Step {
        next_step(m, a, &ITEMS, || cap)
    }

    #[test]
    fn the_subject_follows_the_dialogs() {
        let one = Subject::One(A);
        assert_eq!(
            step(
                &Modal::ConfirmDelete(one),
                ModalAction::Recycle,
                RecycleCapacity::MaxBytes(100)
            ),
            Step::Open(Modal::TooBig {
                subject: one,
                fit: vec![A],
                too_big: vec![B],
            })
        );
        let m = Modal::TooBig {
            subject: one,
            fit: vec![A],
            too_big: vec![B],
        };
        assert_eq!(
            step(&m, ModalAction::TooBigSkip, RecycleCapacity::Unknown),
            Step::Delete(one, vec![(RecycleBin, vec![A])])
        );
    }

    #[test]
    fn recycle_asks_before_anything_too_big() {
        let s = step(
            &Modal::ConfirmDelete(Subject::Basket),
            ModalAction::Recycle,
            RecycleCapacity::MaxBytes(100),
        );
        assert_eq!(
            s,
            Step::Open(Modal::TooBig {
                subject: Subject::Basket,
                fit: vec![A],
                too_big: vec![B],
            })
        );
        let s = step(
            &Modal::ConfirmDelete(Subject::Basket),
            ModalAction::Recycle,
            RecycleCapacity::Disabled,
        );
        assert_eq!(
            s,
            Step::Open(Modal::TooBig {
                subject: Subject::Basket,
                fit: vec![],
                too_big: vec![A, B],
            })
        );
        let s = step(
            &Modal::ConfirmDelete(Subject::Basket),
            ModalAction::Recycle,
            RecycleCapacity::Unknown,
        );
        assert_eq!(
            s,
            Step::Delete(Subject::Basket, vec![(RecycleBin, vec![A, B])])
        );
    }

    #[test]
    fn too_big_choices() {
        let m = Modal::TooBig {
            subject: Subject::Basket,
            fit: vec![A],
            too_big: vec![B],
        };
        let cap = RecycleCapacity::Unknown;
        assert_eq!(
            step(&m, ModalAction::TooBigDeleteAll, cap),
            Step::Delete(
                Subject::Basket,
                vec![(RecycleBin, vec![A]), (Permanent, vec![B])]
            )
        );
        assert_eq!(
            step(&m, ModalAction::TooBigSkip, cap),
            Step::Delete(Subject::Basket, vec![(RecycleBin, vec![A])])
        );
        assert_eq!(step(&m, ModalAction::Close, cap), Step::Close);
        let nothing_fits = Modal::TooBig {
            subject: Subject::Basket,
            fit: vec![],
            too_big: vec![A, B],
        };
        assert_eq!(
            step(&nothing_fits, ModalAction::TooBigSkip, cap),
            Step::Close
        );
        assert_eq!(
            step(&nothing_fits, ModalAction::TooBigDeleteAll, cap),
            Step::Delete(Subject::Basket, vec![(Permanent, vec![A, B])])
        );
    }

    #[test]
    fn permanent_and_empty_bin() {
        let cap = RecycleCapacity::Unknown;
        assert_eq!(
            step(
                &Modal::ConfirmDelete(Subject::Basket),
                ModalAction::DeletePermanently,
                cap
            ),
            Step::Delete(Subject::Basket, vec![(Permanent, vec![A, B])])
        );
        assert_eq!(
            step(&Modal::ConfirmEmptyBin, ModalAction::EmptyBin, cap),
            Step::EmptyBin
        );
        assert_eq!(
            step(
                &Modal::ConfirmDelete(Subject::Basket),
                ModalAction::None,
                cap
            ),
            Step::Stay
        );
        let nothing: [(NodeId, u64); 0] = [];
        assert_eq!(
            next_step(
                &Modal::ConfirmDelete(Subject::Basket),
                ModalAction::DeletePermanently,
                &nothing,
                || cap
            ),
            Step::Close
        );
    }

    #[test]
    fn only_the_matching_confirm_button_acts() {
        use ModalAction as A;
        let failures = Modal::Failures(vec![Failure {
            path: PathBuf::from(r"C:\x"),
            reason: FailureReason::InUse,
        }]);
        let too_big = |subject| Modal::TooBig {
            subject,
            fit: vec![A],
            too_big: vec![B],
        };
        let actions = [
            A::None,
            A::Close,
            A::Recycle,
            A::DeletePermanently,
            A::TooBigDeleteAll,
            A::TooBigSkip,
            A::EmptyBin,
        ];
        let allowed = |m: &Modal, a: ModalAction| {
            matches!(
                (m, a),
                (Modal::ConfirmDelete(_), A::Recycle | A::DeletePermanently)
                    | (Modal::TooBig { .. }, A::TooBigDeleteAll | A::TooBigSkip)
                    | (Modal::ConfirmEmptyBin, A::EmptyBin)
            )
        };
        for m in [
            Modal::ConfirmDelete(Subject::Basket),
            Modal::ConfirmDelete(Subject::One(A)),
            too_big(Subject::Basket),
            too_big(Subject::One(A)),
            Modal::ConfirmEmptyBin,
            failures,
        ] {
            for a in actions {
                for cap in [
                    RecycleCapacity::Unknown,
                    RecycleCapacity::Disabled,
                    RecycleCapacity::MaxBytes(100),
                ] {
                    let s = step(&m, a, cap);
                    if matches!(s, Step::Delete(..) | Step::EmptyBin) {
                        assert!(allowed(&m, a), "{m:?} + {a:?} gave {s:?}");
                    }
                    // Choosing the Recycle Bin never plans a permanent deletion.
                    if a == A::Recycle
                        && let Step::Delete(_, plan) = &s
                    {
                        assert!(plan.iter().all(|(mode, _)| *mode == RecycleBin));
                    }
                }
            }
        }
    }

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn arm_delay_covers_the_double_click_time() {
        assert_eq!(arm_delay_for(None), MS(500));
        assert_eq!(arm_delay_for(Some(MS(200))), MS(300));
        assert_eq!(arm_delay_for(Some(MS(300))), MS(300));
        assert_eq!(arm_delay_for(Some(MS(900))), MS(900));
        assert!(arm_delay() >= MS(300));
    }

    #[test]
    fn buttons_arm_only_for_a_press_that_starts_after_the_delay() {
        let t0 = Instant::now();
        let at = |ms| t0 + MS(ms);
        let d = MS(500);
        // The second press of a double-click: never arms, however late the check.
        assert!(!buttons_armed(t0, at(900), Some(at(200)), d));
        assert!(!buttons_armed(t0, at(5_000), Some(at(499)), d));
        // A press that starts once the delay is over arms.
        assert!(buttons_armed(t0, at(600), Some(at(600)), d));
        assert!(buttons_armed(t0, at(900), Some(at(600)), d));
        assert!(buttons_armed(t0, at(500), Some(at(500)), d));
        // No press, or checked before the delay is over.
        assert!(!buttons_armed(t0, at(900), None, d));
        assert!(!buttons_armed(t0, at(400), Some(at(600)), d));
        // A clock going backwards never arms.
        assert!(!buttons_armed(at(900), t0, Some(t0), d));
    }

    #[test]
    fn unarmed_buttons_can_only_cancel() {
        use ModalAction as A;
        for a in [
            A::Recycle,
            A::DeletePermanently,
            A::TooBigDeleteAll,
            A::TooBigSkip,
            A::EmptyBin,
        ] {
            assert_eq!(gate(a, false), A::None, "{a:?}");
            assert_eq!(gate(a, true), a);
        }
        assert_eq!(gate(A::Close, false), A::Close);
        assert_eq!(gate(A::None, true), A::None);
    }

    #[test]
    fn only_presses_that_start_after_the_delay_count() {
        let t0 = Instant::now();
        let at = |ms| t0 + MS(ms);
        let d = MS(500);
        let mut p = PressTracker::new(t0, d);
        // The opening click, then the second press of a double-click.
        p.track(t0, true);
        p.track(at(200), true);
        p.track(at(499), true);
        p.track(at(600), false);
        assert_eq!(p.first_press(), None);
        assert!(!buttons_armed(t0, at(900), p.first_press(), d));
        p.track(at(600), true);
        p.track(at(700), true);
        assert_eq!(p.first_press(), Some(at(600)));
        assert!(buttons_armed(t0, at(700), p.first_press(), d));
    }
}

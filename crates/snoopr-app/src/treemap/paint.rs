//! Treemap drawing: open folders as neutral containers with a header band,
//! files and closed folders as solid blocks in their type's colour, hover,
//! selection and basket outlines, and the hover tooltip.

use super::model::{
    CachedItem, CachedLayout, LabelMode, LayoutKey, build_layout, hit_test, label_mode,
    needs_rebuild,
};
use crate::i18n::{
    Lang, block_message, denied_dirs, format_age, format_percent, format_size, items_count,
    modified_ago, mostly_line, small_files, type_line, warning_message,
};
use crate::session::Session;
use crate::theme::{
    self, AMBER, HATCH, LOCK_TEXT, TEXT, TEXT_MUTED, Tone, UNSCANNED_FILL, WARN_TEXT,
    container_colors, dimmed, type_color,
};
use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{
    Color32, CursorIcon, FontId, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind,
    Ui, pos2, vec2,
};
use snoopr_core::actions::verdict;
use snoopr_core::age::{AgeFilter, age_months, is_dimmed, now_unix};
use snoopr_core::layout::{self as core_layout, Item};
use snoopr_core::rules::{RuleContext, Verdict};
use snoopr_core::tree::{NodeId, NodeKind};
use std::borrow::Cow;
use std::sync::PoisonError;
use std::time::Instant;

const MARGIN: f32 = 8.0;
const DIR_RADIUS: f32 = 6.0;
const LEAF_RADIUS: f32 = 4.0;
/// Height of a folder's header band (`LayoutParams::default().header_h`).
const HEADER_H: f32 = 16.0;
const HOVER_MIX: f32 = 0.12;
const HATCH_STEP: f32 = 8.0;
/// Opacity of `theme::dimmed` (89 / 255), applied to the labels of dimmed leaves.
const DIM_ALPHA: f32 = 0.35;

pub struct TreemapEvents {
    pub clicked: Option<NodeId>,
    pub double_clicked: Option<NodeId>,
    pub clicked_empty: bool,
}

/// The node a block acts on: a small-files group stands for its parent.
fn target(it: &CachedItem) -> NodeId {
    match it.placed.item {
        Item::Node(id) => id,
        Item::SmallGroup { parent, .. } => parent,
    }
}

/// Cached name, or the localized one for a small-files group, "Not scanned" and the Recycle Bin.
fn display_name(it: &CachedItem, lang: Lang) -> Cow<'_, str> {
    if !it.name.is_empty() {
        return Cow::Borrowed(&it.name);
    }
    match (&it.placed.item, it.tone) {
        (Item::SmallGroup { files, .. }, _) => Cow::Owned(small_files(lang, *files)),
        (_, Tone::Unscanned) => Cow::Borrowed(lang.s().unscanned_name),
        _ => Cow::Borrowed(lang.s().recycle_bin_name),
    }
}

fn to_screen(origin: Pos2, r: core_layout::Rect) -> Rect {
    Rect::from_min_size(origin + vec2(r.x, r.y), vec2(r.w, r.h))
}

fn lit(c: Color32, hot: bool) -> Color32 {
    if hot {
        c.lerp_to_gamma(Color32::WHITE, HOVER_MIX)
    } else {
        c
    }
}

/// One row, cut with "…" at `max_w`.
fn single_row(mut job: LayoutJob, max_w: f32) -> LayoutJob {
    job.wrap = TextWrapping {
        max_width: max_w,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    job
}

fn text_row(p: &Painter, at: Pos2, text: &str, font: FontId, color: Color32, max_w: f32) {
    let job = LayoutJob::simple_singleline(text.to_owned(), font, color);
    p.galley(at, p.layout_job(single_row(job, max_w)), color);
}

/// An open folder (`Dir` with a header band): its children are drawn on top.
fn is_open_dir(it: &CachedItem) -> bool {
    it.kind == NodeKind::Dir && it.placed.header
}

fn paint_dir(p: &Painter, it: &CachedItem, r: Rect, hot: bool, lang: Lang) {
    let c = container_colors(it.container, it.placed.depth);
    p.rect(
        r,
        DIR_RADIUS,
        lit(c.fill, hot),
        Stroke::new(1.0, c.border),
        StrokeKind::Inside,
    );
    let max_w = r.width() - 12.0;
    if max_w <= 0.0 {
        return;
    }
    let mut job = LayoutJob::default();
    job.append(
        &display_name(it, lang),
        0.0,
        TextFormat::simple(theme::bold(10.0), TEXT),
    );
    job.append(
        &format_size(lang, it.size),
        6.0,
        TextFormat::simple(FontId::proportional(10.0), TEXT.gamma_multiply(0.55)),
    );
    let g = p.layout_job(single_row(job, max_w));
    let y = r.top() + (HEADER_H - g.size().y) / 2.0;
    p.with_clip_rect(r).galley(pos2(r.left() + 6.0, y), g, TEXT);
}

/// Diagonal 1 px lines every `step` px, clipped to `r`: the "not scanned" look.
pub fn hatch(p: &Painter, r: Rect, color: Color32, step: f32) {
    let p = p.with_clip_rect(r);
    let st = Stroke::new(1.0, color);
    let h = r.height();
    let mut x = r.left() - h;
    while x < r.right() {
        p.line_segment([pos2(x, r.bottom()), pos2(x + h, r.top())], st);
        x += step;
    }
}

/// Per-frame values shared by leaves and the tooltip.
struct PaintCtx {
    lang: Lang,
    age: AgeFilter,
    /// Unix seconds, read once per frame.
    now: i64,
}

/// A solid block: a file, a small-files group, a closed folder or "not scanned".
fn paint_leaf(p: &Painter, it: &CachedItem, r: Rect, hot: bool, cx: &PaintCtx) {
    let months = age_months(cx.now, it.newest);
    let dim = is_dimmed(cx.age, months);
    let base = match it.tone {
        Tone::Unscanned => UNSCANNED_FILL,
        Tone::Type(t) => type_color(t),
    };
    let alpha = if dim { DIM_ALPHA } else { 1.0 };
    p.rect_filled(
        r,
        LEAF_RADIUS,
        lit(if dim { dimmed(base) } else { base }, hot),
    );
    if it.tone == Tone::Unscanned {
        hatch(p, r, lit(HATCH, hot).gamma_multiply(alpha), HATCH_STEP);
    } else if r.width() > 2.0 * LEAF_RADIUS && r.height() > 2.0 {
        // Light top edge, in place of the mockup's gradient.
        p.hline(
            (r.left() + LEAF_RADIUS)..=(r.right() - LEAF_RADIUS),
            r.top() + 0.5,
            Stroke::new(1.0, Color32::from_white_alpha(26).gamma_multiply(alpha)),
        );
    }
    let white = Color32::WHITE.gamma_multiply(alpha);
    let p = p.with_clip_rect(r);
    match label_mode(r.width(), r.height()) {
        LabelMode::NameAndSize => {
            let max_w = r.width() - 13.0;
            let name = display_name(it, cx.lang);
            text_row(
                &p,
                r.min + vec2(7.0, 5.0),
                &name,
                theme::bold(11.0),
                white,
                max_w,
            );
            let mut sub = format_size(cx.lang, it.size);
            if cx.age != AgeFilter::All {
                sub.push_str(" · ");
                sub.push_str(&format_age(cx.lang, months));
            }
            let font = FontId::proportional(10.0);
            text_row(
                &p,
                r.min + vec2(7.0, 18.75),
                &sub,
                font,
                white.gamma_multiply(0.8),
                max_w,
            );
        }
        LabelMode::NameOnly => {
            let name = display_name(it, cx.lang);
            let max_w = r.width() - 11.0;
            text_row(
                &p,
                r.min + vec2(5.0, 2.0),
                &name,
                theme::bold(9.5),
                white,
                max_w,
            );
        }
        LabelMode::Hidden => {}
    }
}

fn dashed_outline(p: &Painter, r: Rect, color: Color32) {
    let r = r.shrink(1.0);
    let pts = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    p.extend(Shape::dashed_line(&pts, Stroke::new(2.0, color), 5.0, 3.0));
}

fn capitalized(s: String) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => s,
    }
}

/// A guard-rail line: glyph, then the message wrapped in `color`.
fn note(ui: &mut Ui, glyph: &str, color: Color32, msg: &str) {
    ui.add_space(3.0);
    ui.label(
        RichText::new(format!("{glyph} {msg}"))
            .size(11.0)
            .color(color),
    );
}

fn tooltip(ui: &mut Ui, it: &CachedItem, guard: Option<Verdict>, s: &Session, cx: &PaintCtx) {
    let (lang, st) = (cx.lang, cx.lang.s());
    ui.set_max_width(300.0);
    ui.spacing_mut().item_spacing.y = 2.0;
    let small = |t: String| RichText::new(t).size(11.0);
    ui.label(
        RichText::new(display_name(it, lang))
            .font(theme::bold(12.5))
            .color(TEXT),
    );
    let mut line = format_size(lang, it.size);
    let total = s.disk_size();
    if total > 0 {
        let pct = it.size as f64 * 100.0 / total as f64;
        line.push_str(&format!(
            " · {} {}",
            format_percent(lang, pct),
            st.percent_of_disk
        ));
    }
    ui.label(small(line).color(TEXT));
    if it.kind == NodeKind::Unscanned {
        ui.add_space(3.0);
        ui.label(small(st.unscanned_explain.to_owned()).color(TEXT_MUTED));
        if s.denied > 0 {
            ui.label(small(denied_dirs(lang, s.denied)).color(TEXT_MUTED));
        }
        return;
    }
    let is_dir = it.kind == NodeKind::Dir;
    let mut line = capitalized(modified_ago(lang, age_months(cx.now, it.newest)));
    if is_dir {
        line.push_str(" · ");
        line.push_str(&items_count(lang, it.files));
    }
    ui.label(small(line).color(TEXT_MUTED));
    // A file's type, a folder's dominant one; none for a small-files group,
    // whose files may be of any type.
    let kind_line = match (&it.placed.item, it.tone) {
        (Item::SmallGroup { .. }, _) | (_, Tone::Unscanned) => None,
        (Item::Node(_), Tone::Type(_)) if is_dir => {
            it.dominant.map(|(t, share)| mostly_line(lang, t, share))
        }
        (Item::Node(_), Tone::Type(t)) => Some(type_line(lang, t)),
    };
    if let Some(l) = kind_line {
        ui.label(small(l).color(TEXT_MUTED));
    }
    match guard {
        Some(Verdict::Block(b)) => note(ui, "🔒", LOCK_TEXT, block_message(lang, b)),
        Some(Verdict::Warn(w)) => note(ui, "⚠", WARN_TEXT, warning_message(lang, w)),
        _ => {}
    }
    if is_dir {
        ui.add_space(2.0);
        ui.label(RichText::new(st.double_click).size(10.0).color(TEXT_MUTED));
    }
}

/// Draws the treemap of `s.view.root` in the remaining space and reports clicks.
/// Rebuilds `cache` when `needs_rebuild` asks for it, under one `tree.read()`.
pub fn show(
    ui: &mut Ui,
    s: &mut Session,
    cache: &mut Option<CachedLayout>,
    rules: &RuleContext,
    lang: Lang,
) -> TreemapEvents {
    let mut ev = TreemapEvents {
        clicked: None,
        double_clicked: None,
        clicked_empty: false,
    };
    let full = ui.available_rect_before_wrap();
    let resp = ui.allocate_rect(full, Sense::click());
    let area = full.shrink(MARGIN);
    let origin = area.min;
    // Floored, so the layout never spills out of `area`.
    let size = (area.width().max(0.0) as u32, area.height().max(0.0) as u32);
    let local = |p: Pos2| (p.x - origin.x, p.y - origin.y);

    let (hovered, guard) = {
        let tree = s.tree.read().unwrap_or_else(PoisonError::into_inner);
        let key = LayoutKey {
            generation: tree.generation(),
            view_root: s.view.root,
            size,
        };
        let now = Instant::now();
        if needs_rebuild(cache.as_ref(), &key, now, s.is_scanning()) {
            let bounds = core_layout::Rect {
                x: 0.0,
                y: 0.0,
                w: size.0 as f32,
                h: size.1 as f32,
            };
            *cache = Some(build_layout(&tree, key, bounds, now));
        }
        let items = cache.as_ref().map_or(&[][..], |c| &c.items[..]);
        let hovered = resp.hover_pos().and_then(|p| {
            let (x, y) = local(p);
            hit_test(items, x, y)
        });
        // Only the hovered block needs its guard rail, for the tooltip.
        let guard = hovered.and_then(|i| match items[i].placed.item {
            Item::Node(id) if items[i].kind != NodeKind::Unscanned => {
                Some(verdict(&tree, rules, id))
            }
            _ => None,
        });
        (hovered, guard)
    };
    let Some(layout) = cache.as_ref() else {
        return ev;
    };
    let items = &layout.items[..];

    let pick = |p: Pos2| {
        let (x, y) = local(p);
        hit_test(items, x, y).map(|i| target(&items[i]))
    };
    if resp.clicked() {
        match resp.interact_pointer_pos().and_then(pick) {
            Some(id) => ev.clicked = Some(id),
            None => ev.clicked_empty = true,
        }
    }
    if resp.double_clicked() {
        ev.double_clicked = resp.interact_pointer_pos().and_then(pick);
    }

    let painter = ui.painter_at(area);
    let cx = PaintCtx {
        lang,
        age: s.view.age,
        now: now_unix(),
    };
    for (i, it) in items.iter().enumerate() {
        let r = to_screen(origin, it.placed.rect);
        let hot = hovered == Some(i);
        if is_open_dir(it) {
            paint_dir(&painter, it, r, hot, lang);
        } else {
            paint_leaf(&painter, it, r, hot, &cx);
        }
    }
    // Outlines last, so nothing covers them.
    for it in items {
        let Item::Node(id) = it.placed.item else {
            continue;
        };
        let r = to_screen(origin, it.placed.rect);
        if id == s.view.selected {
            let radius = if is_open_dir(it) {
                DIR_RADIUS
            } else {
                LEAF_RADIUS
            };
            painter.rect_stroke(
                r,
                radius,
                Stroke::new(2.0, Color32::WHITE),
                StrokeKind::Inside,
            );
        } else if !s.basket.is_empty() && s.basket.contains(id) {
            dashed_outline(&painter, r, AMBER);
        }
    }

    if let Some(i) = hovered {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        resp.on_hover_ui_at_pointer(|ui| tooltip(ui, &items[i], guard, s, &cx));
    }
    ev
}

//! The single thin top bar (spec §6.2): drive, rescan, back, breadcrumb, age, open, action, basket.

use super::node_label;
use super::widgets::{
    BAR_FONT, Btn, cross, font, logo_square, menu_divider, menu_heading, menu_row, separator,
};
use crate::i18n::{Lang, delete_button, format_size, used_of};
use crate::icons::Icon;
use crate::session::Session;
use crate::theme::{DANGER, TEXT, TEXT_MUTED, bold};
use egui::{Align, Align2, Layout, Rect, RichText, Sense, Ui, UiBuilder, pos2, vec2};
use snoopr_core::actions::{Disabled, PrimaryAction, primary_action};
use snoopr_core::age::AgeFilter;
use snoopr_core::rules::RuleContext;
use snoopr_core::tree::{NodeId, NodeKind, Tree};
use snoopr_core::volume::Volume;
use std::path::PathBuf;
use std::sync::PoisonError;

#[derive(Clone, Debug, PartialEq)]
pub enum TopBarAction {
    None,
    PickVolume(PathBuf),
    PickFolder,
    Rescan,
    StopScan,
    Up,
    ZoomTo(NodeId),
    SetAge(AgeFilter),
    Open,
    Primary,
    RemoveFromBasket(NodeId),
    RequestDelete,
}

const GAP: f32 = 4.0;

/// Two-character drive prefix such as `C:`.
fn drive_letter(root: &std::path::Path) -> String {
    root.to_string_lossy().chars().take(2).collect()
}

fn age_label(lang: Lang, a: AgeFilter) -> &'static str {
    let s = lang.s();
    match a {
        AgeFilter::All => s.age_button_all,
        AgeFilter::Months6 => s.age_6m,
        AgeFilter::Year1 => s.age_1y,
        AgeFilter::Years2 => s.age_2y,
    }
}

/// Label, icon and enabled state of the primary button.
fn primary_look(lang: Lang, p: PrimaryAction) -> (&'static str, Option<Icon>, bool) {
    let s = lang.s();
    match p {
        PrimaryAction::Add => (s.add, Some(Icon::Plus), true),
        PrimaryAction::Remove => (s.remove, Some(Icon::Minus), true),
        PrimaryAction::EmptyRecycleBin => (s.empty_bin, None, true),
        PrimaryAction::Disabled(Disabled::Blocked(_)) => (s.blocked, None, false),
        PrimaryAction::Disabled(Disabled::CoveredBy(_)) => (s.covered, None, false),
        PrimaryAction::Disabled(Disabled::InRecycleBin) => (s.in_bin, None, false),
        PrimaryAction::Disabled(Disabled::Root | Disabled::Unscanned) => {
            (s.add, Some(Icon::Plus), false)
        }
    }
}

/// Ancestors of `from` up to the tree root, root first.
fn crumb_chain(tree: &Tree, from: NodeId) -> Vec<NodeId> {
    let mut chain = vec![from];
    let mut cur = from;
    while let Some(p) = tree.node(cur).parent {
        chain.push(p);
        cur = p;
    }
    chain.reverse();
    chain
}

pub fn show(
    ui: &mut Ui,
    s: &Session,
    volumes: &[Volume],
    rules: &RuleContext,
    lang: Lang,
) -> TopBarAction {
    // One read guard for the whole frame: re-locking while the scan thread waits
    // to write could deadlock.
    let tree = s.tree.read().unwrap_or_else(PoisonError::into_inner);
    let tree = &*tree;
    let full = ui.max_rect();
    let mut action = TopBarAction::None;

    let right = ui.scope_builder(
        UiBuilder::new()
            .max_rect(full)
            .layout(Layout::right_to_left(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            basket_menu(ui, s, tree, lang, &mut action);
            primary_button(ui, s, tree, rules, lang, &mut action);
            open_button(ui, s, tree, lang, &mut action);
            age_menu(ui, s, lang, &mut action);
        },
    );
    let left = ui.scope_builder(
        UiBuilder::new()
            .max_rect(full)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            logo(ui);
            drive_menu(ui, s, volumes, lang, &mut action);
            rescan_button(ui, s, lang, &mut action);
            separator(ui);
            let (resp, clicked) = Btn::new("")
                .icon(Icon::Back)
                .enabled(s.view.root != tree.root())
                .show(ui);
            resp.on_hover_text(lang.s().up);
            if clicked {
                action = TopBarAction::Up;
            }
        },
    );
    let crumb_rect = Rect::from_min_max(
        pos2(left.response.rect.right() + GAP, full.top()),
        pos2(right.response.rect.left() - GAP, full.bottom()),
    );
    if crumb_rect.width() > 20.0 {
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(crumb_rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.set_clip_rect(crumb_rect.intersect(ui.clip_rect()));
                breadcrumb(ui, s, tree, lang, crumb_rect.width(), &mut action);
            },
        );
    }
    action
}

fn logo(ui: &mut Ui) {
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
    logo_square(ui.painter(), rect);
    ui.add_space(3.0);
    let g = ui
        .painter()
        .layout_no_wrap("Snoopr".into(), bold(13.5), TEXT);
    let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
    ui.painter().galley(r.min, g, TEXT);
    ui.add_space(8.0);
}

fn drive_menu(ui: &mut Ui, s: &Session, volumes: &[Volume], lang: Lang, action: &mut TopBarAction) {
    let st = lang.s();
    let letter = drive_letter(&s.volume_root);
    let same = |a: &std::path::Path, b: &std::path::Path| {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    };
    let usage = volumes
        .iter()
        .find(|v| same(&v.root, &s.volume_root))
        .map(|v| (v.total.saturating_sub(v.free), v.total))
        .or_else(|| (s.usage.0 > 0).then(|| (s.usage.0.saturating_sub(s.usage.1), s.usage.0)));
    let muted = usage.map_or(String::new(), |(u, t)| used_of(lang, u, t));
    // Letter in bold then the usage, as two adjacent pieces of one button.
    let label = if muted.is_empty() {
        letter.clone()
    } else {
        format!("{letter}  {muted}")
    };
    let (resp, _) = Btn::new(&label)
        .icon(Icon::Drive)
        .chevron()
        .boxed()
        .show(ui);
    egui::Popup::menu(&resp).width(320.0).show(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        for v in volumes {
            let name = if v.label.is_empty() {
                drive_letter(&v.root)
            } else {
                format!("{}  {}", drive_letter(&v.root), v.label)
            };
            let right = used_of(lang, v.total.saturating_sub(v.free), v.total);
            if menu_row(
                ui,
                Some(Icon::Drive),
                &name,
                &right,
                same(&v.root, &s.volume_root),
            )
            .clicked()
            {
                *action = TopBarAction::PickVolume(v.root.clone());
            }
        }
        menu_divider(ui);
        if menu_row(ui, Some(Icon::Folder), st.choose_folder, "", false).clicked() {
            *action = TopBarAction::PickFolder;
        }
    });
}

fn rescan_button(ui: &mut Ui, s: &Session, lang: Lang, action: &mut TopBarAction) {
    let st = lang.s();
    let (icon, tip, act) = if s.is_scanning() {
        (Icon::Stop, st.stop, TopBarAction::StopScan)
    } else {
        (Icon::Refresh, st.rescan, TopBarAction::Rescan)
    };
    let (resp, clicked) = Btn::new("").icon(icon).show(ui);
    resp.on_hover_text(tip);
    if clicked {
        *action = act;
    }
}

fn breadcrumb(
    ui: &mut Ui,
    s: &Session,
    tree: &Tree,
    lang: Lang,
    width: f32,
    action: &mut TopBarAction,
) {
    let chain = crumb_chain(tree, s.view.root);
    let names: Vec<String> = chain.iter().map(|&id| node_label(tree, id, lang)).collect();
    let p = ui.painter().clone();
    let measure =
        |t: &str, f: egui::FontId| p.layout_no_wrap(t.to_owned(), f, TEXT).size().x + 12.0;
    let last = chain.len() - 1;
    // Keep as many trailing segments as fit; the dropped head becomes "…".
    let mut first = last;
    let mut used = measure(&names[last], bold(BAR_FONT));
    while first > 0 {
        let w = measure(&names[first - 1], font()) + 14.0;
        let ellipsis = if first - 1 > 0 { 30.0 } else { 0.0 };
        if used + w + ellipsis > width {
            break;
        }
        used += w;
        first -= 1;
    }
    ui.spacing_mut().item_spacing.x = 2.0;
    if first > 0 {
        sep_text(ui, "…");
    }
    for i in first..=last {
        if i > first {
            sep_text(ui, "›");
        }
        let is_last = i == last;
        let f = if is_last { bold(BAR_FONT) } else { font() };
        let col = if is_last { TEXT } else { TEXT_MUTED };
        let g = p.layout_no_wrap(names[i].clone(), f, col);
        let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 12.0, 22.0), Sense::click());
        if resp.hovered() && !is_last {
            p.rect_filled(rect, 5.0, egui::Color32::from_rgb(0x1c, 0x1f, 0x27));
        }
        let col = if resp.hovered() { TEXT } else { col };
        p.galley_with_override_text_color(
            pos2(rect.left() + 6.0, rect.center().y - g.size().y / 2.0),
            g,
            col,
        );
        if resp.clicked() && !is_last {
            *action = TopBarAction::ZoomTo(chain[i]);
        }
    }
}

fn sep_text(ui: &mut Ui, t: &str) {
    let g = ui
        .painter()
        .layout_no_wrap(t.into(), font(), TEXT_MUTED.gamma_multiply(0.6));
    let (r, _) = ui.allocate_exact_size(vec2(g.size().x + 2.0, 22.0), Sense::hover());
    ui.painter().galley(
        pos2(r.left() + 1.0, r.center().y - g.size().y / 2.0),
        g,
        TEXT_MUTED,
    );
}

fn age_menu(ui: &mut Ui, s: &Session, lang: Lang, action: &mut TopBarAction) {
    let st = lang.s();
    let active = s.view.age != AgeFilter::All;
    let (resp, _) = Btn::new(age_label(lang, s.view.age))
        .icon(Icon::Clock)
        .chevron()
        .color(if active { crate::theme::ACCENT } else { TEXT })
        .show(ui);
    egui::Popup::menu(&resp)
        .align(egui::RectAlign::BOTTOM_END)
        .width(280.0)
        .show(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            menu_heading(ui, st.age_menu_title);
            for a in AgeFilter::ALL {
                let label = if a == AgeFilter::All {
                    st.age_all
                } else {
                    age_label(lang, a)
                };
                if menu_row(ui, None, label, "", a == s.view.age).clicked() {
                    *action = TopBarAction::SetAge(a);
                }
            }
        });
}

fn open_button(ui: &mut Ui, s: &Session, tree: &Tree, lang: Lang, action: &mut TopBarAction) {
    let openable = tree.node(s.view.selected).kind != NodeKind::Unscanned;
    let (_, clicked) = Btn::new(lang.s().open)
        .icon(Icon::Folder)
        .enabled(openable)
        .show(ui);
    if clicked {
        *action = TopBarAction::Open;
    }
}

fn primary_button(
    ui: &mut Ui,
    s: &Session,
    tree: &Tree,
    rules: &RuleContext,
    lang: Lang,
    action: &mut TopBarAction,
) {
    let p = primary_action(tree, &s.basket, rules, s.view.selected);
    let (label, icon, enabled) = primary_look(lang, p);
    let mut b = Btn::new(label).enabled(enabled);
    if let Some(i) = icon {
        b = b.icon(i);
    }
    let (_, clicked) = b.show(ui);
    if clicked {
        *action = TopBarAction::Primary;
    }
}

fn basket_menu(ui: &mut Ui, s: &Session, tree: &Tree, lang: Lang, action: &mut TopBarAction) {
    let st = lang.s();
    let n = s.basket.len();
    let (resp, _) = Btn::new(st.basket)
        .icon(Icon::Trash)
        .badge(n.to_string(), n > 0)
        .chevron()
        .boxed()
        .show(ui);
    egui::Popup::menu(&resp)
        .align(egui::RectAlign::BOTTOM_END)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(330.0)
        .show(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            menu_heading(ui, st.basket_title);
            if s.basket.is_empty() {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new(st.basket_empty).size(11.0).color(TEXT_MUTED));
                });
                ui.add_space(6.0);
                return;
            }
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for &id in s.basket.items() {
                        basket_row(ui, tree, id, lang, action);
                    }
                });
            menu_divider(ui);
            let total = s.basket.total_size(tree);
            let enabled = !s.is_scanning();
            let (rect, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
            let dim = if enabled { 1.0 } else { 0.35 };
            let p = ui.painter();
            p.rect_filled(rect, 8.0, DANGER.gamma_multiply(dim));
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                delete_button(lang, n, total),
                bold(BAR_FONT),
                TEXT.gamma_multiply(dim),
            );
            let resp = if enabled {
                resp.on_hover_cursor(egui::CursorIcon::PointingHand)
            } else {
                resp.on_hover_text(st.wait_scan)
            };
            if enabled && resp.clicked() {
                *action = TopBarAction::RequestDelete;
            }
        });
}

fn basket_row(ui: &mut Ui, tree: &Tree, id: NodeId, lang: Lang, action: &mut TopBarAction) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    let p = ui.painter().clone();
    if resp.hovered() {
        p.rect_filled(rect, 6.0, egui::Color32::from_rgb(0x1f, 0x23, 0x2c));
    }
    let cy = rect.center().y;
    let x_rect = Rect::from_center_size(pos2(rect.right() - 16.0, cy), vec2(20.0, 20.0));
    let x_resp = ui.interact(
        x_rect,
        ui.scope_id().with(("basket-x", id.0)),
        Sense::click(),
    );
    cross(
        &p,
        x_rect.center(),
        if x_resp.hovered() { TEXT } else { TEXT_MUTED },
    );
    if x_resp.clicked() {
        *action = TopBarAction::RemoveFromBasket(id);
    }
    let size = p.layout_no_wrap(format_size(lang, tree.node(id).size), font(), TEXT_MUTED);
    let size_x = x_rect.left() - 6.0 - size.size().x;
    p.galley(pos2(size_x, cy - size.size().y / 2.0), size, TEXT_MUTED);
    // Name, clipped before the size column.
    let name = p.layout_no_wrap(node_label(tree, id, lang), font(), TEXT);
    let clip = Rect::from_min_max(
        pos2(rect.left() + 8.0, rect.top()),
        pos2(size_x - 8.0, rect.bottom()),
    );
    p.with_clip_rect(clip).galley(
        pos2(rect.left() + 8.0, cy - name.size().y / 2.0),
        name,
        TEXT,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use snoopr_core::rules::Block;

    #[test]
    fn primary_look_table() {
        let s = Lang::En.s();
        let look = |p| primary_look(Lang::En, p);
        let (l, _, en) = look(PrimaryAction::Add);
        assert_eq!((l, en), (s.add, true));
        assert_eq!(look(PrimaryAction::EmptyRecycleBin).0, s.empty_bin);
        let (l, _, en) = look(PrimaryAction::Disabled(Disabled::Blocked(Block::DriveRoot)));
        assert_eq!((l, en), (s.blocked, false));
        assert_eq!(look(PrimaryAction::Disabled(Disabled::Root)).0, s.add);
    }
}

//! Right-click menu of a treemap block (spec animations §2).

use super::node_label;
use super::widgets::{danger_row, menu_divider, menu_heading, menu_note, menu_row};
use crate::i18n::{Lang, block_message, covered_note, format_size};
use crate::icons::Icon;
use crate::session::{Activity, Session};
use egui::Ui;
use snoopr_core::actions::{Disabled, PrimaryAction, primary_action};
use snoopr_core::basket::Basket;
use snoopr_core::rules::{Block, RuleContext};
use snoopr_core::tree::{NodeId, NodeKind, Tree};
use std::sync::PoisonError;

/// What a click in the menu asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    None,
    Zoom(NodeId),
    Reveal(NodeId),
    AddToBasket(NodeId),
    RemoveFromBasket(NodeId),
    Delete(NodeId),
    EmptyBin,
}

/// Why the delete row is greyed out, when there is something to say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    Blocked(Block),
    CoveredBy(NodeId),
    InRecycleBin,
    Scanning,
    Deleting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteRow {
    Delete,
    EmptyBin,
    Disabled(Option<Why>),
}

/// The rows of the menu of `target`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuModel {
    pub target: NodeId,
    /// "Open folder": a folder other than the one shown.
    pub can_zoom: bool,
    /// "Show in File Explorer".
    pub reveal: bool,
    /// The basket row: `Some(true)` removes, `Some(false)` adds, `None` hides it.
    pub in_basket: Option<bool>,
    pub delete: DeleteRow,
}

pub fn menu_model(
    tree: &Tree,
    basket: &Basket,
    rules: &RuleContext,
    view_root: NodeId,
    target: NodeId,
    activity: Activity,
) -> MenuModel {
    if !tree.is_attached(target) {
        return MenuModel {
            target,
            can_zoom: false,
            reveal: false,
            in_basket: None,
            delete: DeleteRow::Disabled(None),
        };
    }
    let (in_basket, mut delete) = match primary_action(tree, basket, rules, target) {
        PrimaryAction::Add => (Some(false), DeleteRow::Delete),
        PrimaryAction::Remove => (Some(true), DeleteRow::Delete),
        PrimaryAction::EmptyRecycleBin => (None, DeleteRow::EmptyBin),
        PrimaryAction::Disabled(d) => (
            None,
            DeleteRow::Disabled(match d {
                Disabled::Blocked(b) => Some(Why::Blocked(b)),
                Disabled::CoveredBy(c) => Some(Why::CoveredBy(c)),
                Disabled::InRecycleBin => Some(Why::InRecycleBin),
                Disabled::Root | Disabled::Unscanned => None,
            }),
        ),
    };
    if matches!(delete, DeleteRow::Delete | DeleteRow::EmptyBin) {
        match activity {
            Activity::Idle => {}
            Activity::Scanning => delete = DeleteRow::Disabled(Some(Why::Scanning)),
            Activity::Deleting { .. } | Activity::EmptyingBin => {
                delete = DeleteRow::Disabled(Some(Why::Deleting));
            }
        }
    }
    let kind = tree.node(target).kind;
    MenuModel {
        target,
        can_zoom: kind == NodeKind::Dir && target != view_root,
        reveal: kind != NodeKind::Unscanned,
        in_basket,
        delete,
    }
}

/// Longest name shown in the heading, which does not wrap.
const HEADING_CHARS: usize = 30;

fn shortened(name: String) -> String {
    if name.chars().count() <= HEADING_CHARS {
        return name;
    }
    let mut out: String = name.chars().take(HEADING_CHARS - 1).collect();
    out.push('…');
    out
}

/// The menu's popup, opened by a right-click on the treemap `response`.
///
/// A right-click on another block while the menu is open moves it there, as
/// Explorer does: `context_menu` reopens it at the pointer, but its default
/// `CloseOnClick` would close it again in the same frame, so that one click
/// is ignored. The rows change with the target, hence the sizing pass that
/// egui runs on any (re)opening.
pub fn popup(response: &egui::Response) -> egui::Popup<'static> {
    let popup = egui::Popup::context_menu(response);
    if response.secondary_clicked() {
        popup
            .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
            .sizing_pass(true)
    } else {
        popup
    }
}

/// Draws the menu of `target` and returns the clicked action (the menu then closes).
pub fn show(
    ui: &mut Ui,
    s: &Session,
    rules: &RuleContext,
    lang: Lang,
    target: NodeId,
) -> MenuAction {
    let st = lang.s();
    let tree = s.tree.read().unwrap_or_else(PoisonError::into_inner);
    let m = menu_model(&tree, &s.basket, rules, s.view.root, target, s.activity());
    ui.set_min_width(230.0);
    ui.set_max_width(300.0);
    let mut action = MenuAction::None;
    let heading = format!(
        "{} · {}",
        shortened(node_label(&tree, target, lang)),
        format_size(lang, tree.node(target).size)
    );
    menu_heading(ui, &heading);
    if m.can_zoom && menu_row(ui, Some(Icon::Folder), st.menu_enter, "", false).clicked() {
        action = MenuAction::Zoom(target);
    }
    if m.reveal && menu_row(ui, None, st.menu_reveal, "", false).clicked() {
        action = MenuAction::Reveal(target);
    }
    menu_divider(ui);
    if let Some(in_basket) = m.in_basket {
        let (icon, label) = if in_basket {
            (Icon::Minus, st.remove)
        } else {
            (Icon::Plus, st.add)
        };
        if menu_row(ui, Some(icon), label, "", false).clicked() {
            action = if in_basket {
                MenuAction::RemoveFromBasket(target)
            } else {
                MenuAction::AddToBasket(target)
            };
        }
    }
    let is_bin = tree.recycle_bin() == Some(target);
    let label = if is_bin {
        st.menu_empty_bin
    } else {
        st.menu_delete
    };
    match m.delete {
        DeleteRow::Delete => {
            if danger_row(ui, Icon::Trash, label, true).clicked() {
                action = MenuAction::Delete(target);
            }
        }
        DeleteRow::EmptyBin => {
            if danger_row(ui, Icon::Trash, st.menu_empty_bin, true).clicked() {
                action = MenuAction::EmptyBin;
            }
        }
        DeleteRow::Disabled(why) => {
            danger_row(ui, Icon::Trash, label, false);
            let note = match why {
                Some(Why::Blocked(b)) => Some(format!("🔒 {}", block_message(lang, b))),
                Some(Why::CoveredBy(c)) => Some(covered_note(lang, &node_label(&tree, c, lang))),
                Some(Why::InRecycleBin) => Some(st.in_bin_note.to_owned()),
                Some(Why::Scanning) => Some(st.busy_scan.to_owned()),
                Some(Why::Deleting) => Some(st.busy_delete.to_owned()),
                None => None,
            };
            if let Some(n) = note {
                menu_note(ui, &n);
            }
        }
    }
    if action != MenuAction::None {
        ui.close();
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Activity::{self, Idle};
    use snoopr_core::basket::Basket;
    use snoopr_core::rules::{Block, RuleContext};
    use snoopr_core::tree::{EntryData, NodeId, NodeKind, Tree};

    fn e(name: &str, kind: NodeKind, size: u64) -> EntryData {
        EntryData {
            name: name.into(),
            kind,
            size,
            newest: 1,
        }
    }

    fn rules() -> RuleContext {
        RuleContext {
            windows_dir: r"C:\Windows".into(),
            program_data: r"C:\ProgramData".into(),
            users_dir: r"C:\Users".into(),
            cloud_roots: Vec::new(),
        }
    }

    struct F {
        t: Tree,
        root: NodeId,
        data: NodeId,
        movie: NodeId,
        windows: NodeId,
        bin: NodeId,
        old: NodeId,
        unscanned: NodeId,
    }

    fn fixture() -> F {
        let mut t = Tree::new(r"C:\".into(), 0);
        let root = t.root();
        let top = t.add_children(
            root,
            vec![
                e("Data", NodeKind::Dir, 0),
                e("Windows", NodeKind::Dir, 0),
                e("$Recycle.Bin", NodeKind::Dir, 0),
            ],
        );
        let (data, windows, bin) = (top[0], top[1], top[2]);
        let movie = t.add_children(data, vec![e("movie.mkv", NodeKind::File, 100)])[0];
        t.add_children(windows, vec![e("w.dll", NodeKind::File, 10)]);
        let old = t.add_children(bin, vec![e("old.zip", NodeKind::File, 5)])[0];
        t.set_unscanned(7);
        let unscanned = t
            .node(root)
            .children
            .iter()
            .copied()
            .find(|&c| t.node(c).kind == NodeKind::Unscanned)
            .unwrap();
        F {
            t,
            root,
            data,
            movie,
            windows,
            bin,
            old,
            unscanned,
        }
    }

    fn model(f: &F, basket: &Basket, view: NodeId, target: NodeId, a: Activity) -> MenuModel {
        menu_model(&f.t, basket, &rules(), view, target, a)
    }

    #[test]
    fn menu_model_file_and_folder() {
        let f = fixture();
        let none = Basket::default();
        let m = model(&f, &none, f.root, f.movie, Idle);
        assert_eq!(
            m,
            MenuModel {
                target: f.movie,
                can_zoom: false,
                reveal: true,
                in_basket: Some(false),
                delete: DeleteRow::Delete,
            }
        );
        let m = model(&f, &none, f.root, f.data, Idle);
        assert!(m.can_zoom && m.delete == DeleteRow::Delete);
        // The folder shown is not offered again.
        assert!(!model(&f, &none, f.data, f.data, Idle).can_zoom);
    }

    #[test]
    fn menu_model_reasons() {
        let f = fixture();
        let none = Basket::default();
        let d = |target| model(&f, &none, f.root, target, Idle).delete;
        assert_eq!(
            d(f.windows),
            DeleteRow::Disabled(Some(Why::Blocked(Block::WindowsDir)))
        );
        assert_eq!(d(f.bin), DeleteRow::EmptyBin);
        assert_eq!(d(f.old), DeleteRow::Disabled(Some(Why::InRecycleBin)));
        assert_eq!(d(f.root), DeleteRow::Disabled(None));
        assert_eq!(d(f.unscanned), DeleteRow::Disabled(None));
        let mut basket = Basket::default();
        basket.add(&f.t, f.data);
        let m = model(&f, &basket, f.root, f.movie, Idle);
        assert_eq!(
            (m.in_basket, m.delete),
            (None, DeleteRow::Disabled(Some(Why::CoveredBy(f.data))))
        );
        assert_eq!(
            model(&f, &basket, f.root, f.data, Idle).in_basket,
            Some(true)
        );
    }

    #[test]
    fn menu_model_busy() {
        let f = fixture();
        let none = Basket::default();
        let d = |target, a| model(&f, &none, f.root, target, a).delete;
        assert_eq!(
            d(f.movie, Activity::Scanning),
            DeleteRow::Disabled(Some(Why::Scanning))
        );
        assert_eq!(
            d(f.bin, Activity::Deleting { done: 0, total: 1 }),
            DeleteRow::Disabled(Some(Why::Deleting))
        );
        assert_eq!(
            d(f.movie, Activity::EmptyingBin),
            DeleteRow::Disabled(Some(Why::Deleting))
        );
    }

    /// Headless egui frames over an 800×600 click area that opens [`popup`].
    struct Harness {
        ctx: egui::Context,
        time: f64,
    }

    impl Harness {
        /// Runs one frame; returns the menu's rectangle when it is shown.
        fn frame(&mut self, events: Vec<egui::Event>) -> Option<egui::Rect> {
            self.time += 0.05;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let mut shown = None;
            let mut out = self.ctx.run_ui(input, |ui| {
                let resp = ui.allocate_rect(ui.max_rect(), egui::Sense::click());
                shown = popup(&resp)
                    .show(|ui| ui.label("menu"))
                    .map(|r| r.response.rect);
            });
            // No renderer: the font atlas is never uploaded.
            out.textures_delta.clear();
            shown
        }

        /// A right-click at `pos`: move, press, release, then one quiet frame.
        fn right_click(&mut self, pos: egui::Pos2) -> Option<egui::Rect> {
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![egui::Event::PointerMoved(pos), button(true)]);
            self.frame(vec![button(false)]);
            self.frame(Vec::new())
        }
    }

    #[test]
    fn a_right_click_elsewhere_moves_the_open_menu() {
        let mut h = Harness {
            ctx: egui::Context::default(),
            time: 0.0,
        };
        assert_eq!(h.frame(Vec::new()), None);
        let a = egui::pos2(100., 100.);
        let at_a = h.right_click(a).expect("opened at the first block");
        assert!((at_a.min - a).length() < 1.0, "{at_a:?}");
        // As in Explorer: the menu reopens on the new block, not just closes.
        let b = egui::pos2(400., 300.);
        let at_b = h.right_click(b).expect("still open, on the new block");
        assert!((at_b.min - b).length() < 1.0, "{at_b:?}");
        // A left click elsewhere still closes it.
        let left = |pressed| egui::Event::PointerButton {
            pos: egui::pos2(600., 500.),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        h.frame(vec![
            egui::Event::PointerMoved(egui::pos2(600., 500.)),
            left(true),
        ]);
        h.frame(vec![left(false)]);
        assert_eq!(h.frame(Vec::new()), None);
    }

    #[test]
    fn menu_model_detached_target() {
        let mut f = fixture();
        f.t.detach(f.movie);
        let m = model(&f, &Basket::default(), f.root, f.movie, Idle);
        assert_eq!(
            m,
            MenuModel {
                target: f.movie,
                can_zoom: false,
                reveal: false,
                in_basket: None,
                delete: DeleteRow::Disabled(None),
            }
        );
    }
}

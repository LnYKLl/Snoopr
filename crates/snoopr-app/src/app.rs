//! Application shell: owns the session and picks the screen to draw.

use crate::i18n::Lang;
use crate::session::{Session, can_start_delete};
use crate::system;
use crate::theme;
use crate::treemap::{self, CachedLayout};
use crate::ui;
use crate::ui::modals::{self, Modal, ModalAction, Step, next_step};
use crate::ui::statusbar::{self, StatusAction};
use crate::ui::topbar::{self, TopBarAction};
use snoopr_core::actions::{PrimaryAction, primary_action};
use snoopr_core::cleanup::DeleteRequest;
use snoopr_core::delete::recycle::recycle_capacity;
use snoopr_core::rules::RuleContext;
use snoopr_core::tree::NodeId;
use snoopr_core::volume::{Volume, list_fixed_volumes};
use std::path::PathBuf;
use std::sync::PoisonError;
use std::time::Duration;

pub struct SnooprApp {
    pub lang: Lang,
    pub rules: RuleContext,
    pub volumes: Vec<Volume>,
    pub session: Option<Session>,
    /// Treemap layout of the current session; reset when the session changes.
    pub layout_cache: Option<CachedLayout>,
    /// Whether the process runs as administrator (queried once).
    pub elevated: bool,
    /// The open deletion dialog; while it is open the rest of the window is inert.
    pub modal: Option<Modal>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Up,
    TogglePrimary,
    CloseModal,
    ZoomSelected,
}

/// Maps a pressed key to its action. While shortcuts are `blocked` (a dialog
/// or a menu is open, or a widget has keyboard focus) only Escape acts.
pub fn key_action(key: egui::Key, blocked: bool) -> Option<KeyAction> {
    use egui::Key;
    match key {
        Key::Escape => Some(KeyAction::CloseModal),
        _ if blocked => None,
        Key::Backspace => Some(KeyAction::Up),
        Key::Delete => Some(KeyAction::TogglePrimary),
        Key::Enter => Some(KeyAction::ZoomSelected),
        _ => None,
    }
}

const KEYS: [egui::Key; 4] = [
    egui::Key::Backspace,
    egui::Key::Delete,
    egui::Key::Escape,
    egui::Key::Enter,
];

impl SnooprApp {
    pub fn new(cc: &eframe::CreationContext, target: Option<PathBuf>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        theme::apply_visuals(&cc.egui_ctx);
        let mut app = SnooprApp {
            lang: Lang::detect(),
            rules: RuleContext::from_system(),
            volumes: list_fixed_volumes(),
            session: None,
            layout_cache: None,
            elevated: system::is_elevated(),
            modal: None,
        };
        if let Some(t) = target {
            app.start_session(t);
        }
        app
    }

    /// Replaces the session, cancelling a scan still running in the old one.
    pub fn start_session(&mut self, target: PathBuf) {
        if let Some(old) = &self.session {
            old.cancel();
        }
        self.session = Some(Session::start(target, &self.rules));
        self.layout_cache = None;
        self.modal = None;
    }

    /// A deletion or the emptying of the Recycle Bin is running.
    fn deleting(&self) -> bool {
        self.session.as_ref().is_some_and(Session::is_deleting)
    }

    /// Adds or removes the selected item, as the primary button does; on the
    /// Recycle Bin, asks to empty it.
    fn toggle_primary(&mut self, ctx: &egui::Context) {
        let Some(s) = &mut self.session else { return };
        let tree = s.tree.clone();
        let tree = tree.read().unwrap_or_else(PoisonError::into_inner);
        let sel = s.view.selected;
        match primary_action(&tree, &s.basket, &self.rules, sel) {
            PrimaryAction::Add => s.basket.add(&tree, sel),
            PrimaryAction::Remove => s.basket.remove(sel),
            PrimaryAction::EmptyRecycleBin => {
                drop(tree);
                self.request_empty_bin(ctx);
            }
            PrimaryAction::Disabled(_) => {}
        }
    }

    /// Opens the deletion confirmation (spec §7.1) when a deletion may start.
    fn request_delete(&mut self, ctx: &egui::Context) {
        let Some(s) = &self.session else { return };
        if self.modal.is_none() && can_start_delete(s.is_busy(), s.basket.len()) {
            egui::Popup::close_all(ctx);
            self.modal = Some(Modal::ConfirmDelete);
        }
    }

    /// Opens the empty-Recycle-Bin confirmation (spec §7.2), even for an empty bin.
    fn request_empty_bin(&mut self, ctx: &egui::Context) {
        let Some(s) = &self.session else { return };
        if self.modal.is_none() && !s.is_busy() {
            egui::Popup::close_all(ctx);
            self.modal = Some(Modal::ConfirmEmptyBin);
        }
    }

    /// Acts on a dialog button. Deleting only ever starts here, from a
    /// confirm button of the open dialog (see [`next_step`]); the session
    /// checks every item again before its worker touches the disk.
    fn apply_modal(&mut self, a: ModalAction) {
        if a == ModalAction::None {
            return;
        }
        let (Some(m), Some(s)) = (&self.modal, &mut self.session) else {
            return;
        };
        let items: Vec<(NodeId, u64)> = s
            .delete_items(s.basket.items())
            .iter()
            .map(|it| (it.id, it.size))
            .collect();
        let volume = s.volume_root.clone();
        match next_step(m, a, &items, || recycle_capacity(&volume)) {
            Step::Stay => {}
            Step::Close => self.modal = None,
            Step::Open(next) => self.modal = Some(next),
            Step::Delete(plan) => {
                self.modal = None;
                let requests = plan
                    .into_iter()
                    .map(|(mode, ids)| DeleteRequest {
                        items: s.delete_items(&ids),
                        mode,
                    })
                    .collect();
                s.start_delete(requests);
            }
            Step::EmptyBin => {
                self.modal = None;
                s.start_empty_bin();
            }
        }
    }

    /// Shows the failures of finished jobs; they wait if another dialog is open.
    fn collect_failures(&mut self) {
        let Some(s) = &mut self.session else { return };
        match &mut self.modal {
            None => {
                let f = s.take_failures();
                if !f.is_empty() {
                    self.modal = Some(Modal::Failures(f));
                }
            }
            Some(Modal::Failures(list)) => list.extend(s.take_failures()),
            Some(_) => {}
        }
    }

    fn handle_key(&mut self, ctx: &egui::Context, action: KeyAction) {
        match action {
            KeyAction::Up => {
                if let Some(s) = &mut self.session {
                    s.up();
                }
            }
            KeyAction::TogglePrimary => self.toggle_primary(ctx),
            KeyAction::ZoomSelected => {
                if let Some(s) = &mut self.session {
                    s.zoom(s.view.selected);
                }
            }
            // Escape: closes the menus and the dialog (Cancel).
            KeyAction::CloseModal => {
                egui::Popup::close_all(ctx);
                self.modal = None;
            }
        }
    }

    fn open_selected(&self) {
        let Some(s) = &self.session else { return };
        let (path, select) = {
            let tree = s.tree.read().unwrap_or_else(PoisonError::into_inner);
            (tree.path(s.view.selected), s.view.selected != tree.root())
        };
        if let Err(e) = system::open_in_explorer(&path, select) {
            eprintln!("snoopr: cannot open Explorer: {e}");
        }
    }

    fn apply_top(&mut self, ctx: &egui::Context, action: TopBarAction) {
        match action {
            TopBarAction::None => {}
            // A running deletion keeps its session: no new scan until it ends.
            TopBarAction::PickVolume(_)
            | TopBarAction::PickFolder
            | TopBarAction::Rescan
            | TopBarAction::StopScan
                if self.deleting() => {}
            TopBarAction::PickVolume(root) => self.start_session(root),
            TopBarAction::PickFolder => {
                // Synchronous dialog: acceptable for a one-off user choice.
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.start_session(p);
                }
            }
            TopBarAction::Rescan => {
                if let Some(target) = self.session.as_ref().map(|s| s.target.clone()) {
                    self.start_session(target);
                }
            }
            TopBarAction::StopScan => {
                if let Some(s) = &self.session {
                    s.cancel();
                }
            }
            TopBarAction::Up => {
                if let Some(s) = &mut self.session {
                    s.up();
                }
            }
            TopBarAction::ZoomTo(id) => {
                if let Some(s) = &mut self.session {
                    s.zoom(id);
                }
            }
            TopBarAction::SetAge(a) => {
                if let Some(s) = &mut self.session {
                    s.view.age = a;
                }
            }
            TopBarAction::Open => self.open_selected(),
            TopBarAction::Primary => self.toggle_primary(ctx),
            TopBarAction::RemoveFromBasket(id) => {
                if let Some(s) = &mut self.session {
                    s.basket.remove(id);
                }
            }
            TopBarAction::RequestDelete => self.request_delete(ctx),
        }
    }

    fn apply_status(&mut self, ctx: &egui::Context, action: StatusAction) {
        match action {
            StatusAction::None => {}
            StatusAction::EmptyBin => self.request_empty_bin(ctx),
            // Relaunching would end the process in the middle of a deletion.
            StatusAction::RelaunchAdmin if self.deleting() => {}
            StatusAction::RelaunchAdmin => {
                if let Some(target) = self.session.as_ref().map(|s| s.target.clone())
                    && system::relaunch_as_admin(&target).is_ok()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
}

/// Flat panel frame of the bars; the border line is painted by the caller.
fn bar_frame(margin_x: i8) -> egui::Frame {
    egui::Frame::NONE
        .fill(theme::BAR_BG)
        .inner_margin(egui::Margin::symmetric(margin_x, 0))
}

impl eframe::App for SnooprApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(s) = &mut self.session {
            s.poll();
            if s.is_busy() {
                // Progress and results arrive from worker threads.
                let ms = if s.is_scanning() { 250 } else { 150 };
                ctx.request_repaint_after(Duration::from_millis(ms));
            }
        }
        self.collect_failures();
        let blocked = self.modal.is_some()
            || egui::Popup::is_any_open(ctx)
            || ctx.egui_wants_keyboard_input();
        for key in KEYS {
            if ctx.input(|i| i.key_pressed(key))
                && let Some(a) = key_action(key, blocked)
            {
                self.handle_key(ctx, a);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // While a dialog is open the bars and the treemap only draw: their
        // input is also blocked by the dialog's backdrop.
        let modal_open = self.modal.is_some();
        let mut picked = None;
        let mut top = TopBarAction::None;
        let mut status = StatusAction::None;
        if let Some(s) = &self.session {
            let r = egui::Panel::top("topbar")
                .exact_size(44.0)
                .frame(bar_frame(8))
                .show(ui, |ui| {
                    topbar::show(ui, s, &self.volumes, &self.rules, self.lang)
                });
            top = r.inner;
            let rect = r.response.rect;
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                egui::Stroke::new(1.0, theme::BORDER),
            );
            let r = egui::Panel::bottom("statusbar")
                .exact_size(30.0)
                .frame(bar_frame(12))
                .show(ui, |ui| {
                    statusbar::show(ui, s, &self.rules, self.lang, self.elevated)
                });
            status = r.inner;
            let rect = r.response.rect;
            ui.painter().hline(
                rect.x_range(),
                rect.top() + 0.5,
                egui::Stroke::new(1.0, theme::BORDER),
            );
            // Shown after the status bar, so it sits just above it, on the
            // treemap background; the chips line up with the treemap's margin.
            egui::Panel::bottom("legend")
                .exact_size(22.0)
                .frame(
                    egui::Frame::NONE
                        .fill(theme::BG)
                        .inner_margin(egui::Margin::symmetric(10, 0)),
                )
                .show(ui, |ui| ui::legend::show(ui, self.lang));
        }
        let panel = if self.session.is_some() {
            // The treemap keeps its own 8 px margin.
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(theme::BG))
        } else {
            egui::CentralPanel::default()
        };
        panel.show(ui, |ui| {
            if let Some(s) = &mut self.session {
                let ev = treemap::show(ui, s, &mut self.layout_cache, &self.rules, self.lang);
                if modal_open {
                    return;
                }
                if let Some(id) = ev.clicked {
                    s.select(id);
                }
                if ev.clicked_empty {
                    s.select(s.view.root);
                }
                if let Some(id) = ev.double_clicked {
                    s.zoom(id);
                }
                if ev.clicked.is_some() || ev.clicked_empty || ev.double_clicked.is_some() {
                    ui.ctx().request_repaint();
                }
            } else {
                picked = ui::start::show(ui, self.lang, &self.volumes);
            }
        });
        if !modal_open {
            if let Some(p) = picked {
                self.start_session(p);
            }
            self.apply_top(ui.ctx(), top);
            self.apply_status(ui.ctx(), status);
        }
        if let (Some(m), Some(s)) = (&self.modal, &self.session) {
            let a = modals::show(ui.ctx(), m, s, &self.rules, self.lang);
            self.apply_modal(a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Key;

    #[test]
    fn keys() {
        assert_eq!(key_action(Key::Backspace, false), Some(KeyAction::Up));
        assert_eq!(
            key_action(Key::Delete, false),
            Some(KeyAction::TogglePrimary)
        );
        assert_eq!(key_action(Key::Escape, true), Some(KeyAction::CloseModal));
        assert_eq!(key_action(Key::Enter, false), Some(KeyAction::ZoomSelected));
        // Blocked by a dialog, a menu or a focused widget: only Escape acts.
        for k in [Key::Backspace, Key::Delete, Key::Enter] {
            assert_eq!(key_action(k, true), None);
        }
    }
}

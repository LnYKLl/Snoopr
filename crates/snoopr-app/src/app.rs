//! Application shell: owns the session and picks the screen to draw.

use crate::i18n::Lang;
use crate::session::Session;
use crate::system;
use crate::theme;
use crate::treemap::{self, CachedLayout};
use crate::ui;
use crate::ui::statusbar::{self, StatusAction};
use crate::ui::topbar::{self, TopBarAction};
use snoopr_core::actions::{PrimaryAction, primary_action};
use snoopr_core::rules::RuleContext;
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Up,
    TogglePrimary,
    CloseModal,
    ZoomSelected,
}

/// Maps a pressed key to its action; while a modal is open only Escape acts.
pub fn key_action(key: egui::Key, modal_open: bool) -> Option<KeyAction> {
    use egui::Key;
    match key {
        Key::Escape => Some(KeyAction::CloseModal),
        _ if modal_open => None,
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
        self.session = Some(Session::start(target));
        self.layout_cache = None;
    }

    /// Adds or removes the selected item, as the primary button does.
    fn toggle_primary(&mut self) {
        let Some(s) = &mut self.session else { return };
        let tree = s.tree.clone();
        let tree = tree.read().unwrap_or_else(PoisonError::into_inner);
        let sel = s.view.selected;
        match primary_action(&tree, &s.basket, &self.rules, sel) {
            PrimaryAction::Add => s.basket.add(&tree, sel),
            PrimaryAction::Remove => s.basket.remove(sel),
            // Task 15: the empty-Recycle-Bin confirmation modal.
            PrimaryAction::EmptyRecycleBin | PrimaryAction::Disabled(_) => {}
        }
    }

    fn handle_key(&mut self, ctx: &egui::Context, action: KeyAction) {
        match action {
            KeyAction::Up => {
                if let Some(s) = &mut self.session {
                    s.up();
                }
            }
            KeyAction::TogglePrimary => self.toggle_primary(),
            KeyAction::ZoomSelected => {
                if let Some(s) = &mut self.session {
                    s.zoom(s.view.selected);
                }
            }
            // Closes open menus; Task 15 adds the modals.
            KeyAction::CloseModal => egui::Popup::close_all(ctx),
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

    fn apply_top(&mut self, action: TopBarAction) {
        match action {
            TopBarAction::None => {}
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
            TopBarAction::Primary => self.toggle_primary(),
            TopBarAction::RemoveFromBasket(id) => {
                if let Some(s) = &mut self.session {
                    s.basket.remove(id);
                }
            }
            TopBarAction::RequestDelete => {
                // Task 15: opens the deletion confirmation.
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
            if s.is_scanning() {
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }
        if !ctx.egui_wants_keyboard_input() {
            // Task 15: becomes true while a modal is open.
            let modal_open = false;
            for key in KEYS {
                if ctx.input(|i| i.key_pressed(key))
                    && let Some(a) = key_action(key, modal_open)
                {
                    self.handle_key(ctx, a);
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
        if let Some(p) = picked {
            self.start_session(p);
        }
        self.apply_top(top);
        match status {
            StatusAction::None => {}
            // Task 15: the empty-Recycle-Bin confirmation modal.
            StatusAction::EmptyBin => {}
            StatusAction::RelaunchAdmin => {
                if let Some(target) = self.session.as_ref().map(|s| s.target.clone())
                    && system::relaunch_as_admin(&target).is_ok()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
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
        assert_eq!(key_action(Key::Delete, true), None);
        assert_eq!(key_action(Key::Escape, true), Some(KeyAction::CloseModal));
        assert_eq!(key_action(Key::Enter, false), Some(KeyAction::ZoomSelected));
    }
}

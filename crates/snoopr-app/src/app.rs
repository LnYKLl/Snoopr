//! Application shell: owns the session and picks the screen to draw.

use crate::i18n::Lang;
use crate::session::Session;
use crate::theme;
use crate::ui;
use snoopr_core::rules::RuleContext;
use snoopr_core::volume::{Volume, list_fixed_volumes};
use std::path::PathBuf;
use std::time::Duration;

pub struct SnooprApp {
    pub lang: Lang,
    #[allow(dead_code)] // read by Task 14
    pub rules: RuleContext,
    pub volumes: Vec<Volume>,
    pub session: Option<Session>,
}

impl SnooprApp {
    pub fn new(cc: &eframe::CreationContext, target: Option<PathBuf>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        theme::apply_visuals(&cc.egui_ctx);
        let mut app = SnooprApp {
            lang: Lang::detect(),
            rules: RuleContext::from_system(),
            volumes: list_fixed_volumes(),
            session: None,
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
    }
}

impl eframe::App for SnooprApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(s) = &mut self.session {
            s.poll();
            if s.is_scanning() {
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut picked = None;
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(s) = &self.session {
                // Treemap and bars arrive with Task 13.
                ui.label(s.target.display().to_string());
            } else {
                picked = ui::start::show(ui, self.lang, &self.volumes);
            }
        });
        if let Some(p) = picked {
            self.start_session(p);
        }
    }
}

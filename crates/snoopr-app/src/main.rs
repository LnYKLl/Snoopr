#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
// Part of the i18n API is consumed by Tasks 13-15.
#[allow(dead_code)]
mod i18n;
// Unused icons and palette entries are consumed by Tasks 13-15.
#[allow(dead_code)]
mod icons;
#[allow(dead_code)]
mod session;
#[allow(dead_code)]
mod theme;
mod ui;

use std::ffi::OsString;
use std::path::PathBuf;

/// First positional argument after the program name.
pub fn parse_target(mut args: impl Iterator<Item = OsString>) -> Option<PathBuf> {
    args.nth(1).map(PathBuf::from)
}

fn main() -> eframe::Result {
    let target = parse_target(std::env::args_os());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Snoopr")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Snoopr",
        options,
        Box::new(move |cc| Ok(Box::new(app::SnooprApp::new(cc, target)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target_takes_first_positional() {
        let args = ["snoopr.exe", r"D:\Data"].map(OsString::from);
        assert_eq!(
            parse_target(args.into_iter()),
            Some(PathBuf::from(r"D:\Data"))
        );
        let args = ["snoopr.exe"].map(OsString::from);
        assert_eq!(parse_target(args.into_iter()), None);
    }
}

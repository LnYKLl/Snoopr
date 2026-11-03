//! Colors, block palettes, visuals and fonts.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke};
use snoopr_core::filetype::FileType;
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(0x0f, 0x11, 0x16);
pub const BAR_BG: Color32 = Color32::from_rgb(0x14, 0x16, 0x1c);
pub const BORDER: Color32 = Color32::from_rgb(0x23, 0x26, 0x2f);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe9, 0xee);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8b, 0x8d, 0x98);
pub const ACCENT: Color32 = Color32::from_rgb(0x3b, 0x6f, 0xe0);
pub const AMBER: Color32 = Color32::from_rgb(0xfb, 0xbf, 0x24);
pub const DANGER: Color32 = Color32::from_rgb(0xc2, 0x36, 0x4a);
pub const LOCK_TEXT: Color32 = Color32::from_rgb(0xf3, 0xb4, 0xb4);
pub const WARN_TEXT: Color32 = Color32::from_rgb(0xf3, 0xd7, 0x9a);
pub const OK_TEXT: Color32 = Color32::from_rgb(0x6e, 0xe7, 0xa8);
/// Opaque fill of tooltips, menus and popups.
pub const POPUP_BG: Color32 = Color32::from_rgb(0x17, 0x1a, 0x21);
/// The "not scanned" block: this fill under `HATCH` diagonal lines.
pub const UNSCANNED_FILL: Color32 = Color32::from_rgb(0x3a, 0x3d, 0x46);
pub const HATCH: Color32 = Color32::from_rgb(0x4e, 0x52, 0x5d);

/// Hue in degrees, saturation and lightness in 0..=100.
pub fn hsl(h: f32, s: f32, l: f32) -> Color32 {
    let (s, l) = (s / 100., l / 100.);
    let c = (1. - (2. * l - 1.).abs()) * s;
    let hp = h.rem_euclid(360.) / 60.;
    let x = c * (1. - (hp % 2. - 1.).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    let m = l - c / 2.;
    let to = |v: f32| ((v + m) * 255.).round().clamp(0., 255.) as u8;
    Color32::from_rgb(to(r), to(g), to(b))
}

/// Fill of a solid block (file, small-files group, closed folder) of type `t`.
pub fn type_color(t: FileType) -> Color32 {
    let (r, g, b) = match t {
        FileType::Game => (0xe1, 0x1d, 0x48),
        FileType::Media => (0x93, 0x33, 0xea),
        FileType::Archive => (0xd9, 0x77, 0x06),
        FileType::VirtualDisk => (0x0d, 0x94, 0x88),
        FileType::Cache => (0x65, 0xa3, 0x0d),
        FileType::Program => (0x25, 0x63, 0xeb),
        FileType::System => (0x47, 0x55, 0x69),
        FileType::Other => (0x64, 0x74, 0x8b),
    };
    Color32::from_rgb(r, g, b)
}

/// Colour of a solid block: its content type, or the hatched "not scanned" grey.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tone {
    Type(FileType),
    Unscanned,
}

/// Look of an open folder: neutral, or dark red in the Recycle Bin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Container {
    Normal,
    RecycleBin,
}

pub struct BlockColors {
    pub fill: Color32,
    pub border: Color32,
}

/// Open-folder colours, slightly lighter at each nesting level.
pub fn container_colors(c: Container, depth: u16) -> BlockColors {
    let d = 3. * f32::from(depth);
    match c {
        Container::Normal => BlockColors {
            fill: hsl(222., 14., (10. + d).min(28.)),
            border: hsl(222., 14., (20. + d).min(40.)),
        },
        Container::RecycleBin => BlockColors {
            fill: hsl(0., 35., (13. + d).min(28.)),
            border: hsl(0., 45., (28. + d).min(44.)),
        },
    }
}

/// 80 % toward grey #2a2d36, at 35 % opacity.
pub fn dimmed(c: Color32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) * 0.2 + f32::from(b) * 0.8).round() as u8;
    Color32::from_rgba_unmultiplied(mix(c.r(), 0x2a), mix(c.g(), 0x2d), mix(c.b(), 0x36), 89)
}

pub fn apply_visuals(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    // Tooltips, menus and popups are framed with `window_fill`: opaque, so the
    // treemap never shows through them.
    v.window_fill = POPUP_BG;
    v.extreme_bg_color = BG;
    v.faint_bg_color = BAR_BG;
    v.window_stroke = Stroke::new(1., BORDER);
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT;
    let r = CornerRadius::same(6);
    let w = &mut v.widgets;
    for (wv, fill) in [
        (&mut w.noninteractive, BAR_BG),
        (&mut w.inactive, BAR_BG),
        (&mut w.hovered, Color32::from_rgb(0x1b, 0x1e, 0x26)),
        (&mut w.active, Color32::from_rgb(0x22, 0x26, 0x30)),
    ] {
        wv.bg_fill = fill;
        wv.weak_bg_fill = fill;
        wv.bg_stroke = Stroke::new(1., BORDER);
        wv.corner_radius = r;
    }
    ctx.set_visuals(v);
}

const BOLD_FAMILY: &str = "bold";

/// Semibold text (egui has no font weights): Segoe UI Semibold, or the regular
/// proportional fonts when it is missing.
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD_FAMILY.into()))
}

/// Segoe UI Variable (or Segoe UI) in front of the proportional family, and the
/// `bold` family used by [`bold`]; never panics.
pub fn install_fonts(ctx: &egui::Context) {
    let read = |paths: &[&str]| paths.iter().find_map(|p| std::fs::read(p).ok());
    let mut fonts = FontDefinitions::default();
    let mut prop = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    if let Some(bytes) = read(&[
        r"C:\Windows\Fonts\SegUIVar.ttf",
        r"C:\Windows\Fonts\segoeui.ttf",
    ]) {
        fonts
            .font_data
            .insert("segoe".to_owned(), Arc::new(FontData::from_owned(bytes)));
        prop.insert(0, "segoe".to_owned());
    }
    // The family must exist even without the font file, or egui panics on use.
    let mut bold = prop.clone();
    if let Some(bytes) = read(&[
        r"C:\Windows\Fonts\seguisb.ttf",
        r"C:\Windows\Fonts\segoeuib.ttf",
    ]) {
        fonts.font_data.insert(
            "segoe-semibold".to_owned(),
            Arc::new(FontData::from_owned(bytes)),
        );
        bold.insert(0, "segoe-semibold".to_owned());
    }
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts
        .families
        .insert(FontFamily::Name(BOLD_FAMILY.into()), bold);
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Color32;

    #[test]
    fn hsl_primaries_and_grey() {
        assert_eq!(hsl(0., 100., 50.), Color32::from_rgb(255, 0, 0));
        let g = hsl(212., 0., 50.);
        assert!(g.r() == 127 || g.r() == 128);
        assert_eq!((g.r(), g.g()), (g.g(), g.b()));
    }

    #[test]
    fn type_colors() {
        use FileType::*;
        let rgb = Color32::from_rgb;
        assert_eq!(type_color(Game), rgb(0xe1, 0x1d, 0x48));
        assert_eq!(type_color(Media), rgb(0x93, 0x33, 0xea));
        assert_eq!(type_color(Archive), rgb(0xd9, 0x77, 0x06));
        assert_eq!(type_color(VirtualDisk), rgb(0x0d, 0x94, 0x88));
        assert_eq!(type_color(Cache), rgb(0x65, 0xa3, 0x0d));
        assert_eq!(type_color(Program), rgb(0x25, 0x63, 0xeb));
        assert_eq!(type_color(System), rgb(0x47, 0x55, 0x69));
        assert_eq!(type_color(Other), rgb(0x64, 0x74, 0x8b));
    }

    #[test]
    fn container_lightness_grows_with_depth_and_is_capped() {
        let n0 = container_colors(Container::Normal, 0);
        assert_eq!(n0.fill, hsl(222., 14., 10.));
        assert_eq!(n0.border, hsl(222., 14., 20.));
        let n2 = container_colors(Container::Normal, 2);
        assert_eq!(n2.fill, hsl(222., 14., 16.));
        assert_eq!(n2.border, hsl(222., 14., 26.));
        let n9 = container_colors(Container::Normal, 9);
        assert_eq!(n9.fill, hsl(222., 14., 28.));
        assert_eq!(n9.border, hsl(222., 14., 40.));
        let b0 = container_colors(Container::RecycleBin, 0);
        assert_eq!(b0.fill, hsl(0., 35., 13.));
        assert_eq!(b0.border, hsl(0., 45., 28.));
        let b9 = container_colors(Container::RecycleBin, 9);
        assert_eq!(b9.fill, hsl(0., 35., 28.));
        assert_eq!(b9.border, hsl(0., 45., 44.));
    }
}

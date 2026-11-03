//! Colors, block palettes, visuals and fonts.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke};
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

pub const HUES: [f32; 8] = [212., 335., 168., 28., 265., 45., 190., 300.];

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tone {
    Normal(f32),
    System,
    RecycleBin,
    Unscanned,
}

pub struct BlockColors {
    pub fill: Color32,
    pub border: Color32,
}

fn hue_sat(t: Tone) -> (f32, f32) {
    match t {
        Tone::Normal(h) => (h, 62.),
        Tone::System => (215., 12.),
        Tone::RecycleBin => (0., 62.),
        Tone::Unscanned => (0., 0.),
    }
}

pub fn dir_colors(t: Tone, depth: u16) -> BlockColors {
    let (h, s) = hue_sat(t);
    let d = f32::from(depth);
    BlockColors {
        fill: hsl(h, s, (13. + 6. * d).min(45.)),
        border: hsl(h, s, (24. + 6. * d).min(60.)),
    }
}

pub fn leaf_color(t: Tone, depth: u16) -> Color32 {
    let (h, s) = hue_sat(t);
    hsl(h, s, (41. - 3. * f32::from(depth)).max(22.))
}

/// 80 % toward grey #2a2d36, at 35 % opacity.
pub fn dimmed(c: Color32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) * 0.2 + f32::from(b) * 0.8).round() as u8;
    Color32::from_rgba_unmultiplied(mix(c.r(), 0x2a), mix(c.g(), 0x2d), mix(c.b(), 0x36), 89)
}

pub fn apply_visuals(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = BAR_BG;
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

/// Segoe UI Variable (or Segoe UI) in front of the proportional family; never panics.
pub fn install_fonts(ctx: &egui::Context) {
    let bytes = [
        r"C:\Windows\Fonts\SegUIVar.ttf",
        r"C:\Windows\Fonts\segoeui.ttf",
    ]
    .iter()
    .find_map(|p| std::fs::read(p).ok());
    let Some(bytes) = bytes else { return };
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert("segoe".to_owned(), Arc::new(FontData::from_owned(bytes)));
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "segoe".to_owned());
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
}

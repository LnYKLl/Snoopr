//! Small custom widgets shared by the top bar and the status bar.

use crate::icons::{Icon, paint_icon};
use crate::theme::{AMBER, LOCK_TEXT, TEXT, TEXT_MUTED, bold};
use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, pos2, vec2};

pub const BAR_FONT: f32 = 12.5;
const BTN_H: f32 = 28.0;
const HOVER_BG: Color32 = Color32::from_rgb(0x1f, 0x23, 0x2c);
const BOX_BG: Color32 = Color32::from_rgb(0x1c, 0x1f, 0x27);
const BOX_BORDER: Color32 = Color32::from_rgb(0x2a, 0x2e, 0x39);
const MENU_HOVER: Color32 = Color32::from_rgb(0x23, 0x27, 0x33);
const MENU_ACTIVE: Color32 = Color32::from_rgb(0x8f, 0xb0, 0xff);

pub fn font() -> FontId {
    FontId::proportional(BAR_FONT)
}

/// A bar button: optional icon, label, badge and drop-down chevron.
pub struct Btn<'a> {
    icon: Option<Icon>,
    label: &'a str,
    chevron: bool,
    boxed: bool,
    enabled: bool,
    color: Color32,
    /// Badge text and whether it is highlighted (amber).
    badge: Option<(String, bool)>,
}

impl<'a> Btn<'a> {
    pub fn new(label: &'a str) -> Self {
        Btn {
            icon: None,
            label,
            chevron: false,
            boxed: false,
            enabled: true,
            color: TEXT,
            badge: None,
        }
    }
    pub fn icon(mut self, i: Icon) -> Self {
        self.icon = Some(i);
        self
    }
    pub fn chevron(mut self) -> Self {
        self.chevron = true;
        self
    }
    pub fn boxed(mut self) -> Self {
        self.boxed = true;
        self
    }
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
    pub fn color(mut self, c: Color32) -> Self {
        self.color = c;
        self
    }
    pub fn badge(mut self, text: String, on: bool) -> Self {
        self.badge = Some((text, on));
        self
    }

    /// Paints the button. The response senses clicks even when disabled (so
    /// hover texts work); the flag is true only for a click on an enabled button.
    pub fn show(self, ui: &mut Ui) -> (Response, bool) {
        let p = ui.painter().clone();
        let dim = if self.enabled { 1.0 } else { 0.35 };
        let col = self.color.gamma_multiply(dim);
        let label =
            (!self.label.is_empty()).then(|| p.layout_no_wrap(self.label.into(), font(), col));
        let badge = self.badge.as_ref().map(|(t, on)| {
            let c = if *on {
                Color32::from_rgb(0x1a, 0x1a, 0x1a)
            } else {
                Color32::from_rgb(0xaa, 0xaa, 0xbb)
            };
            (p.layout_no_wrap(t.clone(), bold(10.5), c), *on)
        });
        let mut w = 16.0;
        if self.icon.is_some() {
            w += 15.0;
        }
        if let Some(l) = &label {
            w += l.size().x + if self.icon.is_some() { 6.0 } else { 0.0 };
        }
        if let Some((b, _)) = &badge {
            w += b.size().x + 12.0 + 6.0;
        }
        if self.chevron {
            w += 12.0 + 6.0;
        }
        let (rect, resp) = ui.allocate_exact_size(vec2(w, BTN_H), Sense::click());
        let hot = resp.hovered() && self.enabled;
        if self.boxed || hot {
            let (fill, stroke) = if self.boxed {
                (
                    if hot { HOVER_BG } else { BOX_BG },
                    Stroke::new(1.0, BOX_BORDER.gamma_multiply(dim)),
                )
            } else {
                (HOVER_BG, Stroke::NONE)
            };
            p.rect(rect, 7.0, fill, stroke, egui::StrokeKind::Inside);
        }
        let mut x = rect.left() + 8.0;
        let cy = rect.center().y;
        if let Some(icon) = self.icon {
            let r = Rect::from_center_size(pos2(x + 7.5, cy), vec2(15.0, 15.0));
            paint_icon(&p, r, icon, col);
            x += 15.0 + 6.0;
        }
        if let Some(l) = label {
            let lw = l.size().x;
            p.galley(pos2(x, cy - l.size().y / 2.0), l, col);
            x += lw + 6.0;
        }
        if let Some((b, on)) = badge {
            let bw = b.size().x + 12.0;
            let br = Rect::from_min_size(pos2(x, cy - 8.0), vec2(bw, 16.0));
            let fill = if on {
                AMBER
            } else {
                Color32::from_rgb(0x2d, 0x32, 0x40)
            };
            p.rect_filled(br, 8.0, fill.gamma_multiply(dim));
            p.galley(pos2(x + 6.0, cy - b.size().y / 2.0), b, TEXT);
            x += bw + 6.0;
        }
        if self.chevron {
            down_chevron(&p, pos2(x + 6.0, cy), col.gamma_multiply(0.6));
        }
        let clicked = resp.clicked() && self.enabled;
        (
            resp.on_hover_cursor(egui::CursorIcon::PointingHand),
            clicked,
        )
    }
}

pub fn down_chevron(p: &egui::Painter, c: Pos2, color: Color32) {
    p.add(Shape::line(
        vec![
            pos2(c.x - 3.5, c.y - 1.5),
            pos2(c.x, c.y + 2.0),
            pos2(c.x + 3.5, c.y - 1.5),
        ],
        Stroke::new(1.5, color),
    ));
}

pub fn separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 20.0), Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(1.0, BOX_BORDER),
    );
}

/// A check mark centered on `c`.
fn check_mark(p: &egui::Painter, c: Pos2, color: Color32) {
    p.add(Shape::line(
        vec![
            pos2(c.x - 4.0, c.y),
            pos2(c.x - 1.0, c.y + 3.0),
            pos2(c.x + 4.5, c.y - 3.5),
        ],
        Stroke::new(1.6, color),
    ));
}

/// A cross centered on `c`.
pub fn cross(p: &egui::Painter, c: Pos2, color: Color32) {
    let st = Stroke::new(1.4, color);
    p.line_segment([pos2(c.x - 3.5, c.y - 3.5), pos2(c.x + 3.5, c.y + 3.5)], st);
    p.line_segment([pos2(c.x - 3.5, c.y + 3.5), pos2(c.x + 3.5, c.y - 3.5)], st);
}

/// Small uppercase heading of a menu.
pub fn menu_heading(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    ui.painter().text(
        rect.left_center() + vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        text.to_uppercase(),
        FontId::proportional(10.0),
        TEXT_MUTED,
    );
}

pub fn menu_divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(
        rect.x_range().shrink(4.0),
        rect.center().y,
        Stroke::new(1.0, BOX_BORDER),
    );
}

/// A menu row: `left` text, optional muted `right` text, check mark when `active`.
pub fn menu_row(
    ui: &mut Ui,
    icon: Option<Icon>,
    left: &str,
    right: &str,
    active: bool,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 6.0, MENU_HOVER);
    }
    let col = if active { MENU_ACTIVE } else { TEXT };
    let cy = rect.center().y;
    let mut x = rect.left() + 10.0;
    if let Some(i) = icon {
        let r = Rect::from_center_size(pos2(x + 7.5, cy), vec2(15.0, 15.0));
        paint_icon(p, r, i, TEXT_MUTED);
        x += 23.0;
    }
    let g = p.layout_no_wrap(left.into(), font(), col);
    let gh = g.size().y;
    p.galley(pos2(x, cy - gh / 2.0), g, col);
    let mut rx = rect.right() - 10.0;
    if active {
        check_mark(p, pos2(rx - 4.0, cy), col);
        rx -= 16.0;
    }
    if !right.is_empty() {
        p.text(
            pos2(rx, cy),
            Align2::RIGHT_CENTER,
            right,
            font(),
            TEXT_MUTED,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A destructive menu row (red text, icon in the same colour). Disabled, it is
/// dimmed and reacts neither to hover nor with a hand cursor.
pub fn danger_row(ui: &mut Ui, icon: Icon, label: &str, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), sense);
    let p = ui.painter();
    let col = if enabled {
        LOCK_TEXT
    } else {
        TEXT_MUTED.gamma_multiply(0.5)
    };
    if enabled && resp.hovered() {
        p.rect_filled(rect, 6.0, MENU_HOVER);
    }
    let cy = rect.center().y;
    let x = rect.left() + 10.0;
    paint_icon(
        p,
        Rect::from_center_size(pos2(x + 7.5, cy), vec2(15.0, 15.0)),
        icon,
        col,
    );
    let g = p.layout_no_wrap(label.into(), font(), col);
    let gh = g.size().y;
    p.galley(pos2(x + 23.0, cy - gh / 2.0), g, col);
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// A small grey explanation under a menu row; wraps, indented by 10 px.
pub fn menu_note(ui: &mut Ui, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.add_space(10.0);
        ui.label(egui::RichText::new(text).size(11.0).color(TEXT_MUTED));
    });
}

/// The 15 px gradient square of the logo.
pub fn logo_square(p: &egui::Painter, rect: Rect) {
    let (a, b) = ([0x5b_u8, 0x8d, 0xef], [0xa7_u8, 0x8b, 0xfa]);
    let r = 4.0_f32;
    let mut pts = Vec::new();
    for (corner, start) in [
        (pos2(rect.right() - r, rect.top() + r), -90.0_f32),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (pos2(rect.left() + r, rect.top() + r), 180.0),
    ] {
        for i in 0..=3 {
            let ang = (start + 30.0 * i as f32).to_radians();
            pts.push(corner + vec2(ang.cos(), ang.sin()) * r);
        }
    }
    let color_at = |q: Pos2| {
        let t = (((q.x - rect.left()) + (q.y - rect.top())) / (rect.width() + rect.height()))
            .clamp(0.0, 1.0);
        let mix =
            |i: usize| (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t).round() as u8;
        Color32::from_rgb(mix(0), mix(1), mix(2))
    };
    let mut mesh = egui::epaint::Mesh::default();
    let center = rect.center();
    mesh.colored_vertex(center, color_at(center));
    for q in &pts {
        mesh.colored_vertex(*q, color_at(*q));
    }
    let n = pts.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(Shape::mesh(mesh));
}

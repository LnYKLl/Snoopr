//! Line icons drawn with the painter (1.6 px strokes, like the SVGs of the mockup).

use egui::{Color32, Painter, Pos2, Rect, Stroke, pos2};

#[allow(dead_code)] // the toolbar (Task 14) draws them
#[derive(Clone, Copy)]
pub enum Icon {
    Drive,
    Refresh,
    Stop,
    Back,
    Clock,
    Folder,
    Plus,
    Minus,
    Trash,
    Chevron,
    Lock,
    Warn,
}

pub fn paint_icon(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let st = Stroke::new(1.6, color);
    let c = rect.center();
    let u = rect.width().min(rect.height()) / 16.0;
    // Maps a point in a 16x16 design grid to screen space.
    let m = |x: f32, y: f32| pos2(c.x + (x - 8.0) * u, c.y + (y - 8.0) * u);
    let line = |pts: &[(f32, f32)]| {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| m(x, y)).collect();
        p.add(egui::Shape::line(v, st));
    };
    let rrect = |x0: f32, y0: f32, x1: f32, y1: f32, r: f32| {
        p.rect_stroke(
            Rect::from_min_max(m(x0, y0), m(x1, y1)),
            r * u,
            st,
            egui::StrokeKind::Middle,
        );
    };
    match icon {
        Icon::Drive => {
            rrect(2.0, 9.0, 14.0, 13.5, 1.5);
            line(&[(2.5, 9.0), (4.5, 3.5), (11.5, 3.5), (13.5, 9.0)]);
            p.circle_filled(m(11.5, 11.2), 0.9 * u, color);
        }
        Icon::Refresh => {
            let pts: Vec<(f32, f32)> = (0..=10)
                .map(|i| {
                    let a = (40.0 + 280.0 * i as f32 / 10.0).to_radians();
                    (8.0 + 5.5 * a.cos(), 8.0 - 5.5 * a.sin())
                })
                .collect();
            line(&pts);
            line(&[(13.0, 2.0), (13.5, 5.0), (10.5, 5.2)]);
        }
        Icon::Stop => rrect(3.5, 3.5, 12.5, 12.5, 2.0),
        Icon::Back => line(&[(13.0, 8.0), (3.5, 8.0), (7.5, 4.0), (3.5, 8.0), (7.5, 12.0)]),
        Icon::Clock => {
            p.circle_stroke(m(8.0, 8.0), 6.0 * u, st);
            line(&[(8.0, 4.5), (8.0, 8.0), (10.5, 9.5)]);
        }
        Icon::Folder => line(&[
            (2.0, 4.5),
            (2.0, 12.5),
            (14.0, 12.5),
            (14.0, 5.5),
            (7.5, 5.5),
            (6.5, 3.5),
            (2.0, 3.5),
            (2.0, 4.5),
        ]),
        Icon::Plus => {
            line(&[(8.0, 3.0), (8.0, 13.0)]);
            line(&[(3.0, 8.0), (13.0, 8.0)]);
        }
        Icon::Minus => line(&[(3.0, 8.0), (13.0, 8.0)]),
        Icon::Trash => {
            line(&[(2.5, 4.0), (13.5, 4.0)]);
            line(&[(6.0, 4.0), (6.0, 2.5), (10.0, 2.5), (10.0, 4.0)]);
            line(&[(4.0, 4.0), (4.7, 13.5), (11.3, 13.5), (12.0, 4.0)]);
            line(&[(6.6, 7.0), (6.8, 11.0)]);
            line(&[(9.4, 7.0), (9.2, 11.0)]);
        }
        Icon::Chevron => line(&[(6.0, 3.5), (10.5, 8.0), (6.0, 12.5)]),
        Icon::Lock => {
            rrect(3.5, 7.0, 12.5, 13.5, 1.5);
            line(&[
                (5.5, 7.0),
                (5.5, 5.0),
                (6.2, 3.4),
                (8.0, 2.7),
                (9.8, 3.4),
                (10.5, 5.0),
                (10.5, 7.0),
            ]);
        }
        Icon::Warn => {
            line(&[(8.0, 2.5), (14.0, 13.0), (2.0, 13.0), (8.0, 2.5)]);
            line(&[(8.0, 6.5), (8.0, 9.5)]);
            p.circle_filled(m(8.0, 11.3), 0.8 * u, color);
        }
    }
}

//! The "freed" pill: appears over the treemap when the session's freed total
//! grows, counts up from 0 to what was just freed, holds, then fades out.

use crate::anim::{ease_out_cubic, progress};
use crate::i18n::{Lang, freed};
use crate::theme::{BORDER, OK_TEXT, POPUP_BG, TEXT};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, Id, LayerId, Order, Rect, Stroke, StrokeKind, Ui, vec2};
use std::time::{Duration, Instant};

const APPEAR: Duration = Duration::from_millis(200);
const COUNT: Duration = Duration::from_millis(700);
const HOLD: Duration = Duration::from_millis(3500);
const FADE: Duration = Duration::from_millis(400);
/// How far below its rest position the pill starts, in px.
const RISE: f32 = 12.0;

/// What to draw for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToastView {
    pub value: u64,
    pub alpha: f32,
    /// Pixels below the rest position (12 → 0).
    pub dy: f32,
}

pub struct FreedToast {
    last_freed: u64,
    /// The session total when the pill last became visible: the pill shows
    /// what was freed since then, counting from 0.
    base: u64,
    /// The value to count up to (`last_freed - base`).
    target: u64,
    from_value: u64,
    count_start: Instant,
    /// When the pill last became visible (not reset by a bump while visible).
    appear: Option<Instant>,
    last_bump: Option<Instant>,
    enabled: bool,
}

impl FreedToast {
    pub fn new(freed: u64, enabled: bool) -> Self {
        FreedToast {
            last_freed: freed,
            base: freed,
            target: 0,
            from_value: 0,
            count_start: Instant::now(),
            appear: None,
            last_bump: None,
            enabled,
        }
    }

    /// Called every frame with the session's total freed bytes.
    pub fn update(&mut self, freed: u64, now: Instant) {
        if freed < self.last_freed {
            // A new session: nothing to show, just start over from here.
            *self = FreedToast::new(freed, self.enabled);
            return;
        }
        if freed == self.last_freed {
            return;
        }
        let was_visible = self.view(now).is_some();
        if was_visible {
            self.from_value = self.value_at(now);
        } else {
            self.base = self.last_freed;
            self.from_value = 0;
        }
        self.last_freed = freed;
        self.target = freed - self.base;
        self.count_start = now;
        self.last_bump = Some(now);
        if !was_visible {
            self.appear = Some(now);
        }
    }

    fn value_at(&self, now: Instant) -> u64 {
        if !self.enabled {
            return self.target;
        }
        let p = ease_out_cubic(progress(self.count_start, now, COUNT)) as f64;
        let (a, b) = (self.from_value as f64, self.target as f64);
        (a + (b - a) * p).round() as u64
    }

    pub fn view(&self, now: Instant) -> Option<ToastView> {
        let bump = self.last_bump?;
        let since = now.saturating_duration_since(bump);
        if !self.enabled {
            return (since < HOLD).then_some(ToastView {
                value: self.target,
                alpha: 1.0,
                dy: 0.0,
            });
        }
        if since >= HOLD + FADE {
            return None;
        }
        let fade_out = if since > HOLD {
            1.0 - progress(bump + HOLD, now, FADE)
        } else {
            1.0
        };
        let appear = self.appear.map_or(1.0, |a| progress(a, now, APPEAR));
        Some(ToastView {
            value: self.value_at(now),
            alpha: appear.min(fade_out),
            dy: RISE * (1.0 - ease_out_cubic(appear)),
        })
    }

    /// True while the pill is visible.
    pub fn is_animating(&self, now: Instant) -> bool {
        self.view(now).is_some()
    }
}

/// Paints the pill centred, 16 px above the bottom of `area`. It lives on the
/// middle layer: over the panels, under the dialogs and menus.
pub fn paint(ui: &Ui, area: Rect, v: ToastView, lang: Lang) {
    let fade = |c: Color32| c.gamma_multiply(v.alpha);
    let font = FontId::proportional(13.5);
    let text = freed(lang, v.value);
    let mut job = LayoutJob::default();
    // The text starts with "✓ ": colour the tick apart from the rest.
    let (tick, rest) = text.split_at(text.char_indices().nth(1).map_or(text.len(), |(i, _)| i));
    for (part, color) in [(tick, OK_TEXT), (rest, TEXT)] {
        job.append(
            part,
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: fade(color),
                ..Default::default()
            },
        );
    }
    let painter = ui
        .ctx()
        .layer_painter(LayerId::new(Order::Middle, Id::unique("freed_toast")));
    let galley = painter.layout_job(job);
    let size = galley.size() + vec2(28.0, 16.0);
    let center = egui::pos2(area.center().x, area.bottom() - 16.0 - size.y / 2.0 + v.dy);
    let rect = Rect::from_center_size(center, size);
    painter.rect(
        rect,
        size.y / 2.0,
        fade(POPUP_BG),
        Stroke::new(1.0, fade(BORDER)),
        StrokeKind::Inside,
    );
    painter.galley(
        Align2::CENTER_CENTER
            .align_size_within_rect(galley.size(), rect)
            .min,
        galley,
        Color32::WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.02
    }

    #[test]
    fn nothing_without_a_rise() {
        let t0 = Instant::now();
        let mut t = FreedToast::new(0, true);
        t.update(0, t0);
        assert_eq!(t.view(t0), None);
        assert!(!t.is_animating(t0));
    }

    #[test]
    fn first_rise_timeline() {
        let t0 = Instant::now();
        let mut t = FreedToast::new(0, true);
        t.update(1000, t0);
        let v = t.view(t0).unwrap();
        assert_eq!((v.alpha, v.dy), (0.0, 12.0));
        let v = t.view(t0 + ms(200)).unwrap();
        assert_eq!((v.alpha, v.dy), (1.0, 0.0));
        let v = t.view(t0 + ms(350)).unwrap();
        assert!(0 < v.value && v.value < 1000);
        assert_eq!(t.view(t0 + ms(700)).unwrap().value, 1000);
        assert_eq!(t.view(t0 + ms(3500)).unwrap().alpha, 1.0);
        assert!(near(t.view(t0 + ms(3700)).unwrap().alpha, 0.5));
        assert_eq!(t.view(t0 + ms(3900)), None);
        assert!(t.is_animating(t0 + ms(100)));
        assert!(!t.is_animating(t0 + ms(3900)));
    }

    #[test]
    fn second_rise_extends_and_counts_from_the_shown_value() {
        let t0 = Instant::now();
        let mut t = FreedToast::new(0, true);
        t.update(1000, t0);
        t.update(1500, t0 + ms(1000));
        assert_eq!(t.target, 1500);
        assert_eq!(t.view(t0 + ms(1000)).unwrap().value, 1000);
        assert_eq!(t.view(t0 + ms(1700)).unwrap().value, 1500);
        let v = t.view(t0 + ms(4400)).unwrap();
        assert_eq!((v.alpha, v.dy), (1.0, 0.0));
    }

    #[test]
    fn a_new_pill_counts_what_was_just_freed_from_zero() {
        let t0 = Instant::now();
        let mut t = FreedToast::new(0, true);
        t.update(1000, t0);
        // The first pill has vanished by then.
        assert_eq!(t.view(t0 + ms(5000)), None);
        t.update(1500, t0 + ms(5000));
        assert_eq!(t.view(t0 + ms(5000)).unwrap().value, 0);
        assert_eq!(t.view(t0 + ms(5700)).unwrap().value, 500);
    }

    #[test]
    fn disabled_motion_shows_at_once_and_vanishes() {
        let t0 = Instant::now();
        let mut t = FreedToast::new(0, false);
        t.update(1000, t0);
        assert_eq!(
            t.view(t0),
            Some(ToastView {
                value: 1000,
                alpha: 1.0,
                dy: 0.0
            })
        );
        assert_eq!(t.view(t0 + ms(3500)), None);
    }

    #[test]
    fn a_decrease_shows_nothing() {
        let t0 = Instant::now();
        let mut t = FreedToast::new(5000, true);
        t.update(0, t0);
        assert_eq!(t.view(t0), None);
        // Re-based: a later rise counts from the new base.
        t.update(100, t0 + ms(10));
        assert!(t.view(t0 + ms(10)).is_some());
    }
}

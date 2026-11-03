//! Treemap transitions: blocks slide from what is on screen to their new place,
//! vanished blocks fade out as ghosts, and a change of root zooms like a camera.
//! Pure: no painting, time comes in as `now`.

use super::model::{CachedItem, CachedLayout};
use crate::anim::{ease_in_out_cubic, ease_out_cubic, progress};
use snoopr_core::layout::{Item, Rect};
use snoopr_core::tree::{NodeId, Tree};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Same folder shown: blocks slide, appear and vanish.
pub const REFLOW: Duration = Duration::from_millis(260);
/// Into a folder or back up: the camera move.
pub const ZOOM: Duration = Duration::from_millis(340);
/// Any other change of root, or a first layout: a cross-fade.
pub const APPEAR: Duration = Duration::from_millis(200);
/// A block that appears grows from this share of its size.
pub const SPAWN_SCALE: f32 = 0.85;
/// Above this many blocks plus ghosts, layouts change without animation.
pub const MAX_ANIMATED: usize = 6000;

pub fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    let mix = |a: f32, b: f32| a + (b - a) * t;
    Rect {
        x: mix(a.x, b.x),
        y: mix(a.y, b.y),
        w: mix(a.w, b.w),
        h: mix(a.h, b.h),
    }
}

/// `r` placed inside `to` as it sits inside `from`. A zero-size `from`
/// collapses everything onto `to`'s corner.
pub fn map_rect(r: Rect, from: Rect, to: Rect) -> Rect {
    let sx = if from.w > 0.0 { to.w / from.w } else { 0.0 };
    let sy = if from.h > 0.0 { to.h / from.h } else { 0.0 };
    Rect {
        x: to.x + (r.x - from.x) * sx,
        y: to.y + (r.y - from.y) * sy,
        w: r.w * sx,
        h: r.h * sy,
    }
}

/// `r` scaled by `k` around its centre; `k = 0` gives a point at the centre.
pub fn scale_about_center(r: Rect, k: f32) -> Rect {
    let (w, h) = (r.w * k, r.h * k);
    Rect {
        x: r.x + (r.w - w) / 2.0,
        y: r.y + (r.h - h) / 2.0,
        w,
        h,
    }
}

/// Identity of a block across layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimKey {
    Node(NodeId),
    /// The small-files group of that parent.
    Group(NodeId),
}

impl AnimKey {
    pub fn of(item: &Item) -> AnimKey {
        match *item {
            Item::Node(id) => AnimKey::Node(id),
            Item::SmallGroup { parent, .. } => AnimKey::Group(parent),
        }
    }
}

/// How a new layout relates to the previous one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    /// Window resized: no animation.
    Snap,
    /// First layout, or an unrelated root: cross-fade.
    Appear,
    /// Same root: blocks move to their new place.
    Reflow,
    /// The new root is below the old one; it grows from `focus_old`.
    ZoomIn { focus_old: Rect },
    /// The new root is above the old one, which shrinks into `focus_new`.
    ZoomOut { focus_new: Rect },
}

/// `id` or its deepest ancestor below `stop` placed in `l`, with its
/// rectangle there. The view root itself is never placed.
fn focus(tree: &Tree, l: &CachedLayout, id: NodeId, stop: NodeId) -> Option<(NodeId, Rect)> {
    std::iter::successors(Some(id), |&n| tree.node(n).parent)
        .take_while(|&n| n != stop)
        .find_map(|n| {
            l.items
                .iter()
                .find(|it| it.placed.item == Item::Node(n))
                .map(|it| (n, it.placed.rect))
        })
}

pub fn classify_change(
    tree: &Tree,
    old: Option<&CachedLayout>,
    new: &CachedLayout,
    area: Rect,
) -> Change {
    let Some(old) = old else {
        return Change::Appear;
    };
    if old.key.size != new.key.size {
        return Change::Snap;
    }
    let (from, to) = (old.key.view_root, new.key.view_root);
    let rect = |f: Option<(NodeId, Rect)>| f.map_or(area, |(_, r)| r);
    if from == to {
        Change::Reflow
    } else if tree.is_ancestor(from, to) {
        Change::ZoomIn {
            focus_old: rect(focus(tree, old, to, from)),
        }
    } else if tree.is_ancestor(to, from) {
        Change::ZoomOut {
            focus_new: rect(focus(tree, new, from, to)),
        }
    } else {
        Change::Appear
    }
}

/// Where and how opaque a block is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub rect: Rect,
    pub alpha: f32,
}

fn mix(a: Shown, b: Shown, t: f32) -> Shown {
    Shown {
        rect: lerp_rect(a.rect, b.rect, t),
        alpha: a.alpha + (b.alpha - a.alpha) * t,
    }
}

/// Start, length and curve of a transition.
#[derive(Clone, Copy)]
struct Clock {
    start: Instant,
    duration: Duration,
    ease: fn(f32) -> f32,
}

impl Clock {
    /// Eased progress at `now`; `None` once finished.
    fn eased(&self, now: Instant) -> Option<f32> {
        let p = progress(self.start, now, self.duration);
        (p < 1.0).then(|| (self.ease)(p))
    }
}

/// A block of the previous layout, drawn while it fades out.
struct Ghost {
    item: CachedItem,
    from: Shown,
    to: Shown,
}

struct Transition {
    clock: Clock,
    /// Where each key of the new layout starts; it ends at its target, opaque.
    from: HashMap<AnimKey, Shown>,
    ghosts: Vec<Ghost>,
}

impl Transition {
    fn shown(&self, key: AnimKey, target: Rect, now: Instant) -> Shown {
        let end = Shown {
            rect: target,
            alpha: 1.0,
        };
        // Once finished, no lookup: the idle path is a single clock check.
        let Some(t) = self.clock.eased(now) else {
            return end;
        };
        match self.from.get(&key) {
            Some(&from) => mix(from, end, t),
            None => end,
        }
    }
}

/// Where a block that vanished in a reflow goes: into its deepest ancestor
/// placed in the new layout when it is still in the tree, else onto its own
/// centre.
fn reflow_exit(tree: &Tree, key: AnimKey, start: Rect, targets: &HashMap<AnimKey, Rect>) -> Shown {
    let into = match key {
        AnimKey::Node(id) if tree.is_attached(id) => {
            std::iter::successors(tree.node(id).parent, |&a| tree.node(a).parent)
                .find_map(|a| targets.get(&AnimKey::Node(a)).copied())
        }
        _ => None,
    };
    Shown {
        rect: scale_about_center(into.unwrap_or(start), 0.0),
        alpha: 0.0,
    }
}

/// The treemap's running transition, if any. Hit testing never looks at it:
/// it always uses the final layout.
pub struct Animator {
    enabled: bool,
    transition: Option<Transition>,
}

impl Animator {
    pub fn new(enabled: bool) -> Self {
        Animator {
            enabled,
            transition: None,
        }
    }

    pub fn reset(&mut self) {
        self.transition = None;
    }

    /// Starts the transition from what is on screen at `now` to `new`. Call before `old` is replaced.
    pub fn on_rebuild(
        &mut self,
        tree: &Tree,
        old: Option<&CachedLayout>,
        new: &CachedLayout,
        area: Rect,
        now: Instant,
    ) {
        let mut prev = self.transition.take();
        if !self.enabled || new.items.len() > MAX_ANIMATED {
            return;
        }
        let mut change = classify_change(tree, old, new, area);
        let (duration, ease): (Duration, fn(f32) -> f32) = match change {
            Change::Snap => return,
            Change::Reflow => (REFLOW, ease_out_cubic),
            Change::ZoomIn { .. } | Change::ZoomOut { .. } => (ZOOM, ease_in_out_cubic),
            Change::Appear => (APPEAR, ease_out_cubic),
        };
        // Ghosts still visible keep going from where they are to where they were going.
        let mut ghosts = Vec::new();
        if let Some(tr) = prev.as_mut()
            && let Some(t) = tr.clock.eased(now)
        {
            ghosts.extend(std::mem::take(&mut tr.ghosts).into_iter().filter_map(|g| {
                let from = mix(g.from, g.to, t);
                (from.alpha > 0.0).then_some(Ghost { from, ..g })
            }));
        }
        let budget = MAX_ANIMATED - new.items.len();
        if ghosts.len() > budget {
            return;
        }
        // What is on screen now for a block of `old`.
        let depart = |key: AnimKey, rect: Rect| match &prev {
            Some(tr) => tr.shown(key, rect, now),
            None => Shown { rect, alpha: 1.0 },
        };
        // A zoom in starts from the folder as drawn now, even mid-reflow. Out,
        // the focus is in `new`, which is drawn as laid out.
        if let (Change::ZoomIn { focus_old }, Some(old)) = (&mut change, old)
            && let Some((id, rect)) = focus(tree, old, new.key.view_root, old.key.view_root)
        {
            *focus_old = depart(AnimKey::Node(id), rect).rect;
        }
        let old_items = old.map_or(&[][..], |o| &o.items[..]);
        let from = if change == Change::Reflow {
            let targets: HashMap<AnimKey, Rect> = new
                .items
                .iter()
                .map(|it| (AnimKey::of(&it.placed.item), it.placed.rect))
                .collect();
            let mut from = HashMap::with_capacity(new.items.len());
            // Kept blocks that move and new blocks; ghosts are checked below.
            let mut moved = false;
            for it in old_items {
                let key = AnimKey::of(&it.placed.item);
                let start = depart(key, it.placed.rect);
                if let Some(&target) = targets.get(&key) {
                    moved |= start.rect != target || start.alpha != 1.0;
                    from.insert(key, start);
                    continue;
                }
                ghosts.push(Ghost {
                    item: it.clone(),
                    from: start,
                    to: reflow_exit(tree, key, start.rect, &targets),
                });
                if ghosts.len() > budget {
                    return;
                }
            }
            for it in &new.items {
                from.entry(AnimKey::of(&it.placed.item)).or_insert_with(|| {
                    moved = true;
                    Shown {
                        rect: scale_about_center(it.placed.rect, SPAWN_SCALE),
                        alpha: 0.0,
                    }
                });
            }
            if !moved && ghosts.is_empty() {
                return;
            }
            from
        } else {
            if ghosts.len() + old_items.len() > budget {
                return;
            }
            // Zooms: the new layout comes in from `enter`, the old one leaves
            // to `exit`. A cross-fade stays in place.
            let (enter, exit) = match change {
                Change::ZoomIn { focus_old: f } => (Some((area, f)), Some((f, area))),
                Change::ZoomOut { focus_new: f } => (Some((f, area)), Some((area, f))),
                _ => (None, None),
            };
            let warp = |r: Rect, m: Option<(Rect, Rect)>| m.map_or(r, |(a, b)| map_rect(r, a, b));
            for it in old_items {
                let start = depart(AnimKey::of(&it.placed.item), it.placed.rect);
                ghosts.push(Ghost {
                    item: it.clone(),
                    from: start,
                    to: Shown {
                        rect: warp(start.rect, exit),
                        alpha: 0.0,
                    },
                });
            }
            new.items
                .iter()
                .map(|it| {
                    let start = Shown {
                        rect: warp(it.placed.rect, enter),
                        alpha: 0.0,
                    };
                    (AnimKey::of(&it.placed.item), start)
                })
                .collect()
        };
        self.transition = Some(Transition {
            clock: Clock {
                start: now,
                duration,
                ease,
            },
            from,
            ghosts,
        });
    }

    pub fn shown(&self, key: AnimKey, target: Rect, now: Instant) -> Shown {
        match &self.transition {
            Some(tr) => tr.shown(key, target, now),
            None => Shown {
                rect: target,
                alpha: 1.0,
            },
        }
    }

    /// Vanished blocks still visible at `now`, in drawing order.
    pub fn ghosts(&self, now: Instant) -> Vec<(&CachedItem, Shown)> {
        let Some(tr) = &self.transition else {
            return Vec::new();
        };
        let Some(t) = tr.clock.eased(now) else {
            return Vec::new();
        };
        tr.ghosts
            .iter()
            .map(|g| (&g.item, mix(g.from, g.to, t)))
            .filter(|(_, s)| s.alpha > 0.0)
            .collect()
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.transition
            .as_ref()
            .is_some_and(|tr| tr.clock.eased(now).is_some())
    }
}

/// Hover highlight fade duration.
pub const HOVER: Duration = Duration::from_millis(120);
/// Selection outline glide duration.
pub const GLIDE: Duration = Duration::from_millis(160);

/// Hover highlight levels: the hovered key rises to 1, the others fall to 0 and are dropped.
pub struct HoverFade {
    enabled: bool,
    levels: Vec<(AnimKey, f32)>,
}

impl HoverFade {
    pub fn new(enabled: bool) -> Self {
        HoverFade {
            enabled,
            levels: Vec::new(),
        }
    }

    /// Moves every level by `dt` seconds towards its goal.
    pub fn update(&mut self, hovered: Option<AnimKey>, dt: f32) {
        if !self.enabled {
            self.levels.clear();
            self.levels.extend(hovered.map(|k| (k, 1.0)));
            return;
        }
        if let Some(k) = hovered
            && !self.levels.iter().any(|(l, _)| *l == k)
        {
            self.levels.push((k, 0.0));
        }
        let step = dt / HOVER.as_secs_f32();
        for (k, v) in &mut self.levels {
            *v = if Some(*k) == hovered {
                (*v + step).min(1.0)
            } else {
                (*v - step).max(0.0)
            };
        }
        self.levels.retain(|(_, v)| *v > 1e-4);
    }

    pub fn level(&self, key: AnimKey) -> f32 {
        self.levels
            .iter()
            .find(|(k, _)| *k == key)
            .map_or(0.0, |(_, v)| *v)
    }

    /// Some level is still between 0 and 1.
    pub fn is_animating(&self) -> bool {
        self.levels.iter().any(|(_, v)| *v < 1.0 - 1e-4)
    }
}

/// The selection outline: glides from the previous selection to the new one.
pub struct Glide {
    enabled: bool,
    key: Option<AnimKey>,
    /// Where the outline was when the selection changed.
    from: Option<Rect>,
    start: Instant,
    /// What was returned last, the start of the next glide.
    last: Option<Rect>,
}

impl Glide {
    pub fn new(enabled: bool) -> Self {
        Glide {
            enabled,
            key: None,
            from: None,
            start: Instant::now(),
            last: None,
        }
    }

    /// Where to draw the outline of `key`, whose block is at `target` now.
    /// Call once per frame.
    pub fn rect(&mut self, key: Option<AnimKey>, target: Rect, now: Instant) -> Rect {
        if !self.enabled {
            return target;
        }
        if key != self.key {
            self.from = if key.is_some() { self.last } else { None };
            self.start = now;
            self.key = key;
        }
        let shown = match (self.from, key) {
            (Some(from), Some(_)) => {
                let p = progress(self.start, now, GLIDE);
                lerp_rect(from, target, ease_out_cubic(p))
            }
            _ => target,
        };
        self.last = key.map(|_| shown);
        shown
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.enabled
            && self.from.is_some()
            && self.key.is_some()
            && progress(self.start, now, GLIDE) < 1.0
    }
}

/// Everything the treemap animates, owned by the app and reset with the session.
pub struct TreemapAnim {
    pub layout: Animator,
    pub hover: HoverFade,
    pub glide: Glide,
    pub last_frame: Option<Instant>,
}

impl TreemapAnim {
    pub fn new(enabled: bool) -> Self {
        TreemapAnim {
            layout: Animator::new(enabled),
            hover: HoverFade::new(enabled),
            glide: Glide::new(enabled),
            last_frame: None,
        }
    }

    pub fn reset(&mut self) {
        self.layout.reset();
        self.hover.levels.clear();
        self.glide.key = None;
        self.glide.from = None;
        self.glide.last = None;
        self.last_frame = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Container, Tone};
    use crate::treemap::model::LayoutKey;
    use snoopr_core::filetype::FileType;
    use snoopr_core::layout::{Placed, Rect as R};
    use snoopr_core::tree::{EntryData, NodeKind};
    use std::path::PathBuf;

    const AREA: R = R {
        x: 0.,
        y: 0.,
        w: 800.,
        h: 600.,
    };
    const SIZE: (u32, u32) = (800, 600);

    fn r(x: f32, y: f32, w: f32, h: f32) -> R {
        R { x, y, w, h }
    }

    fn near_eps(a: R, b: R, eps: f32) -> bool {
        (a.x - b.x).abs() < eps
            && (a.y - b.y).abs() < eps
            && (a.w - b.w).abs() < eps
            && (a.h - b.h).abs() < eps
    }

    fn near(a: R, b: R) -> bool {
        near_eps(a, b, 1e-4)
    }

    fn assert_close(got: Shown, want: Shown) {
        assert!(
            near(got.rect, want.rect) && (got.alpha - want.alpha).abs() < 1e-4,
            "{got:?} != {want:?}"
        );
    }

    fn ci(item: Item, rect: R) -> CachedItem {
        CachedItem {
            placed: Placed {
                item,
                rect,
                depth: 0,
                header: false,
            },
            tone: Tone::Type(FileType::Other),
            container: Container::Normal,
            dominant: None,
            name: String::new(),
            size: 1,
            newest: 0,
            files: 1,
            kind: NodeKind::File,
        }
    }

    fn node(id: NodeId, rect: R) -> CachedItem {
        ci(Item::Node(id), rect)
    }

    fn lay(view_root: NodeId, size: (u32, u32), items: Vec<CachedItem>) -> CachedLayout {
        CachedLayout {
            key: LayoutKey {
                generation: 0,
                view_root,
                size,
            },
            built_at: Instant::now(),
            items,
        }
    }

    fn e(name: &str, kind: NodeKind, size: u64) -> EntryData {
        EntryData {
            name: name.into(),
            kind,
            size,
            newest: 0,
        }
    }

    struct Ids {
        root: NodeId,
        bin: NodeId,
        users: NodeId,
        alice: NodeId,
        a: NodeId,
        b: NodeId,
        docs: NodeId,
        windows: NodeId,
        dll: NodeId,
    }

    /// `C:\` with `$Recycle.Bin`, `Users\alice\{a.zip, b.zip, docs\c.txt}`
    /// and `Windows\w.dll`.
    fn tree() -> (Tree, Ids) {
        let mut t = Tree::new(PathBuf::from(r"C:\"), 0);
        let root = t.root();
        let top = t.add_children(
            root,
            vec![
                e("$Recycle.Bin", NodeKind::Dir, 0),
                e("Users", NodeKind::Dir, 0),
                e("Windows", NodeKind::Dir, 0),
            ],
        );
        let (bin, users, windows) = (top[0], top[1], top[2]);
        let alice = t.add_children(users, vec![e("alice", NodeKind::Dir, 0)])[0];
        let k = t.add_children(
            alice,
            vec![
                e("a.zip", NodeKind::File, 100),
                e("b.zip", NodeKind::File, 50),
                e("docs", NodeKind::Dir, 0),
            ],
        );
        let (a, b, docs) = (k[0], k[1], k[2]);
        t.add_children(docs, vec![e("c.txt", NodeKind::File, 25)]);
        let dll = t.add_children(windows, vec![e("w.dll", NodeKind::File, 10)])[0];
        let ids = Ids {
            root,
            bin,
            users,
            alice,
            a,
            b,
            docs,
            windows,
            dll,
        };
        (t, ids)
    }

    fn ghost_of(anim: &Animator, item: &Item, now: Instant) -> Option<Shown> {
        anim.ghosts(now)
            .into_iter()
            .find(|(it, _)| it.placed.item == *item)
            .map(|(_, s)| s)
    }

    #[test]
    fn timings_match_the_spec() {
        assert_eq!(REFLOW, Duration::from_millis(260));
        assert_eq!(ZOOM, Duration::from_millis(340));
        assert_eq!(APPEAR, Duration::from_millis(200));
        assert_eq!(SPAWN_SCALE, 0.85);
        assert_eq!(MAX_ANIMATED, 6000);
    }

    #[test]
    fn rect_math() {
        let r = R {
            x: 50.,
            y: 50.,
            w: 50.,
            h: 50.,
        };
        assert_eq!(
            map_rect(
                r,
                R {
                    x: 0.,
                    y: 0.,
                    w: 100.,
                    h: 100.
                },
                R {
                    x: 10.,
                    y: 20.,
                    w: 50.,
                    h: 50.
                }
            ),
            R {
                x: 35.,
                y: 45.,
                w: 25.,
                h: 25.
            }
        );
        assert_eq!(
            lerp_rect(
                R {
                    x: 0.,
                    y: 0.,
                    w: 10.,
                    h: 10.
                },
                R {
                    x: 10.,
                    y: 20.,
                    w: 30.,
                    h: 10.
                },
                0.5
            ),
            R {
                x: 5.,
                y: 10.,
                w: 20.,
                h: 10.
            }
        );
        assert_eq!(
            scale_about_center(
                R {
                    x: 0.,
                    y: 0.,
                    w: 10.,
                    h: 20.
                },
                0.0
            ),
            R {
                x: 5.,
                y: 10.,
                w: 0.,
                h: 0.
            }
        );
        // A degenerate source collapses instead of dividing by zero.
        let point = R {
            x: 10.,
            y: 10.,
            w: 0.,
            h: 0.,
        };
        let m = map_rect(r, point, AREA);
        assert!(m.x.is_finite() && m.y.is_finite() && m.w == 0. && m.h == 0.);
    }

    #[test]
    fn animkey_of_items() {
        assert_eq!(
            AnimKey::of(&Item::Node(NodeId(4))),
            AnimKey::Node(NodeId(4))
        );
        let group = Item::SmallGroup {
            parent: NodeId(4),
            files: 3,
            size: 9,
        };
        assert_eq!(AnimKey::of(&group), AnimKey::Group(NodeId(4)));
    }

    #[test]
    fn classify_change_table() {
        let (t, ids) = tree();
        let (u, al, w) = (
            r(0., 0., 600., 600.),
            r(3., 19., 594., 578.),
            r(600., 0., 200., 600.),
        );
        let at = |root, items| lay(root, SIZE, items);
        let cl =
            |old: Option<&CachedLayout>, new: &CachedLayout| classify_change(&t, old, new, AREA);

        let base = at(ids.root, vec![node(ids.users, u)]);
        assert_eq!(cl(None, &base), Change::Appear);
        // Another size snaps, even for the same root or a zoom.
        let wider = lay(ids.root, (801, 600), vec![node(ids.users, u)]);
        assert_eq!(cl(Some(&base), &wider), Change::Snap);
        let wider_zoom = lay(ids.alice, (801, 600), vec![]);
        assert_eq!(cl(Some(&base), &wider_zoom), Change::Snap);
        assert_eq!(cl(Some(&base), &at(ids.root, vec![])), Change::Reflow);

        // Zoom in: the new root placed, else its deepest placed ancestor, else the area.
        let old = at(ids.root, vec![node(ids.users, u), node(ids.alice, al)]);
        let into_alice = at(ids.alice, vec![]);
        let into_docs = at(ids.docs, vec![]);
        assert_eq!(
            cl(Some(&old), &into_alice),
            Change::ZoomIn { focus_old: al }
        );
        assert_eq!(cl(Some(&old), &into_docs), Change::ZoomIn { focus_old: al });
        let only_users = at(ids.root, vec![node(ids.users, u)]);
        assert_eq!(
            cl(Some(&only_users), &into_docs),
            Change::ZoomIn { focus_old: u }
        );
        let only_windows = at(ids.root, vec![node(ids.windows, w)]);
        assert_eq!(
            cl(Some(&only_windows), &into_alice),
            Change::ZoomIn { focus_old: AREA }
        );

        // Zoom out: the old root in the new layout, or its deepest placed ancestor.
        let from_alice = at(ids.alice, vec![]);
        let from_docs = at(ids.docs, vec![]);
        let up = at(ids.root, vec![node(ids.users, u), node(ids.alice, al)]);
        assert_eq!(
            cl(Some(&from_alice), &up),
            Change::ZoomOut { focus_new: al }
        );
        assert_eq!(cl(Some(&from_docs), &up), Change::ZoomOut { focus_new: al });
        let up_users = at(ids.root, vec![node(ids.users, u)]);
        assert_eq!(
            cl(Some(&from_docs), &up_users),
            Change::ZoomOut { focus_new: u }
        );
        let up_windows = at(ids.root, vec![node(ids.windows, w)]);
        assert_eq!(
            cl(Some(&from_alice), &up_windows),
            Change::ZoomOut { focus_new: AREA }
        );

        // Sibling roots cross-fade.
        let users_view = at(ids.users, vec![]);
        let windows_view = at(ids.windows, vec![]);
        assert_eq!(cl(Some(&users_view), &windows_view), Change::Appear);
    }

    #[test]
    fn reflow_moves_kept_blocks_and_spawns_new_ones() {
        let (t, ids) = tree();
        let (u1, w1) = (r(0., 0., 400., 600.), r(400., 0., 400., 600.));
        let (u2, w2, b2) = (
            r(0., 0., 600., 600.),
            r(600., 0., 200., 300.),
            r(600., 300., 200., 300.),
        );
        let old = lay(
            ids.root,
            SIZE,
            vec![node(ids.users, u1), node(ids.windows, w1)],
        );
        let new = lay(
            ids.root,
            SIZE,
            vec![
                node(ids.users, u2),
                node(ids.windows, w2),
                node(ids.bin, b2),
            ],
        );
        let mut anim = Animator::new(true);
        let t0 = Instant::now();
        anim.on_rebuild(&t, Some(&old), &new, AREA, t0);
        assert!(anim.is_animating(t0));

        // Kept blocks start where they were…
        let users = AnimKey::Node(ids.users);
        let windows = AnimKey::Node(ids.windows);
        assert_eq!(
            anim.shown(users, u2, t0),
            Shown {
                rect: u1,
                alpha: 1.0
            }
        );
        assert_eq!(
            anim.shown(windows, w2, t0),
            Shown {
                rect: w1,
                alpha: 1.0
            }
        );
        // …are strictly between halfway, on an ease-out curve…
        let half = t0 + REFLOW / 2;
        let mid = anim.shown(users, u2, half);
        assert!(mid.rect.w > 400. && mid.rect.w < 600.);
        assert_eq!(mid.alpha, 1.0);
        assert_close(
            mid,
            Shown {
                rect: lerp_rect(u1, u2, 0.875),
                alpha: 1.0,
            },
        );
        let mid_w = anim.shown(windows, w2, half);
        assert!(mid_w.rect.x > 400. && mid_w.rect.x < 600.);
        // …and end at their new place.
        let end = t0 + REFLOW;
        assert_eq!(
            anim.shown(users, u2, end),
            Shown {
                rect: u2,
                alpha: 1.0
            }
        );
        assert_eq!(
            anim.shown(windows, w2, end),
            Shown {
                rect: w2,
                alpha: 1.0
            }
        );

        // A new block grows from 85 % of its size, transparent.
        let bin = AnimKey::Node(ids.bin);
        assert_eq!(
            anim.shown(bin, b2, t0),
            Shown {
                rect: scale_about_center(b2, 0.85),
                alpha: 0.0
            }
        );
        let mid_b = anim.shown(bin, b2, half);
        assert!(mid_b.alpha > 0.0 && mid_b.alpha < 1.0);
        assert_eq!(
            anim.shown(bin, b2, end),
            Shown {
                rect: b2,
                alpha: 1.0
            }
        );

        assert!(!anim.is_animating(end));
        assert!(anim.ghosts(t0).is_empty());
        assert!(anim.ghosts(end).is_empty());
    }

    #[test]
    fn idle_reflow_does_not_animate() {
        let (t, ids) = tree();
        let items = || vec![node(ids.users, r(0., 0., 400., 600.))];
        let mut anim = Animator::new(true);
        let t0 = Instant::now();
        anim.on_rebuild(
            &t,
            Some(&lay(ids.root, SIZE, items())),
            &lay(ids.root, SIZE, items()),
            AREA,
            t0,
        );
        assert!(!anim.is_animating(t0));

        // Nothing else moves, but a block vanishes: its ghost still fades out.
        let u = r(0., 0., 400., 600.);
        let b = r(400., 0., 400., 600.);
        let old = lay(ids.root, SIZE, vec![node(ids.users, u), node(ids.bin, b)]);
        let new = lay(ids.root, SIZE, vec![node(ids.users, u)]);
        anim.on_rebuild(&t, Some(&old), &new, AREA, t0);
        assert!(anim.is_animating(t0));
        assert_eq!(
            ghost_of(&anim, &Item::Node(ids.bin), t0),
            Some(Shown {
                rect: b,
                alpha: 1.0
            })
        );
    }

    #[test]
    fn zoom_in_mid_reflow_starts_from_the_drawn_folder() {
        let (t, ids) = tree();
        let (u1, al1) = (r(0., 0., 400., 600.), r(3., 19., 394., 578.));
        let (u2, al2) = (r(0., 0., 800., 600.), r(3., 19., 794., 578.));
        let l1 = lay(
            ids.root,
            SIZE,
            vec![node(ids.users, u1), node(ids.alice, al1)],
        );
        let l2 = lay(
            ids.root,
            SIZE,
            vec![node(ids.users, u2), node(ids.alice, al2)],
        );
        let mut anim = Animator::new(true);
        let t0 = Instant::now();
        anim.on_rebuild(&t, Some(&l1), &l2, AREA, t0);

        let t1 = t0 + Duration::from_millis(100);
        let drawn = anim.shown(AnimKey::Node(ids.alice), al2, t1).rect;
        assert!(!near(drawn, al2), "alice is still moving");
        let before_u = anim.shown(AnimKey::Node(ids.users), u2, t1);
        let a3 = r(0., 0., 500., 600.);
        let l3 = lay(ids.alice, SIZE, vec![node(ids.a, a3)]);
        anim.on_rebuild(&t, Some(&l2), &l3, AREA, t1);

        // The new treemap starts inside alice as drawn, not as laid out…
        assert_close(
            anim.shown(AnimKey::Node(ids.a), a3, t1),
            Shown {
                rect: map_rect(a3, AREA, drawn),
                alpha: 0.0,
            },
        );
        // …and the old one leaves in step with it, from where it is drawn.
        let gu = ghost_of(&anim, &Item::Node(ids.users), t1).expect("fading");
        assert_close(gu, before_u);
        let late = t1 + ZOOM - Duration::from_millis(2);
        let gu = ghost_of(&anim, &Item::Node(ids.users), late).expect("fading");
        let out = map_rect(before_u.rect, drawn, AREA);
        assert!(near_eps(gu.rect, out, 1e-1), "{gu:?} vs {out:?}");
    }

    #[test]
    fn deleted_blocks_dissolve_and_recycled_ones_fly_into_the_bin() {
        let (mut t, ids) = tree();
        let (ra, rb, rg) = (
            r(103., 19., 400., 578.),
            r(503., 19., 200., 578.),
            r(703., 19., 94., 578.),
        );
        let group = Item::SmallGroup {
            parent: ids.alice,
            files: 3,
            size: 3,
        };
        let old = lay(
            ids.root,
            SIZE,
            vec![
                node(ids.bin, r(0., 0., 100., 600.)),
                node(ids.users, r(100., 0., 700., 600.)),
                node(ids.alice, r(102., 17., 696., 581.)),
                node(ids.a, ra),
                node(ids.b, rb),
                ci(group.clone(), rg),
            ],
        );
        t.detach(ids.a); // deleted for good
        t.reparent(ids.b, ids.bin); // sent to the Recycle Bin
        let bin2 = r(0., 0., 300., 600.);
        let new = lay(
            ids.root,
            SIZE,
            vec![
                node(ids.bin, bin2),
                node(ids.users, r(300., 0., 500., 600.)),
                node(ids.alice, r(302., 17., 496., 581.)),
            ],
        );
        let mut anim = Animator::new(true);
        let t0 = Instant::now();
        anim.on_rebuild(&t, Some(&old), &new, AREA, t0);
        assert_eq!(anim.ghosts(t0).len(), 3);
        let (a, b) = (Item::Node(ids.a), Item::Node(ids.b));
        // Ghosts start where the blocks were, opaque.
        assert_eq!(
            ghost_of(&anim, &a, t0),
            Some(Shown {
                rect: ra,
                alpha: 1.0
            })
        );
        assert_eq!(
            ghost_of(&anim, &b, t0),
            Some(Shown {
                rect: rb,
                alpha: 1.0
            })
        );
        assert_eq!(
            ghost_of(&anim, &group, t0),
            Some(Shown {
                rect: rg,
                alpha: 1.0
            })
        );

        let late = t0 + REFLOW - Duration::from_millis(2);
        // The detached file shrinks to its own centre (not into alice, still placed).
        let ga = ghost_of(&anim, &a, late).expect("still fading");
        assert!(
            near_eps(ga.rect, scale_about_center(ra, 0.0), 1e-2),
            "{ga:?}"
        );
        assert!(ga.alpha < 0.01);
        // The recycled file flies to the centre of the bin's new rectangle.
        let gb = ghost_of(&anim, &b, late).expect("still fading");
        assert!(
            near_eps(gb.rect, scale_about_center(bin2, 0.0), 1e-2),
            "{gb:?}"
        );
        assert!(gb.alpha < 0.01);
        // A small-files group always dissolves in place.
        let gg = ghost_of(&anim, &group, late).expect("still fading");
        assert!(
            near_eps(gg.rect, scale_about_center(rg, 0.0), 1e-2),
            "{gg:?}"
        );

        assert!(anim.ghosts(t0 + REFLOW).is_empty());
        assert!(!anim.is_animating(t0 + REFLOW));
    }

    #[test]
    fn zoom_in_grows_from_the_folder() {
        let (t, ids) = tree();
        let (u, al, w) = (
            r(0., 0., 600., 600.),
            r(3., 19., 594., 578.),
            r(600., 0., 200., 600.),
        );
        let old = lay(
            ids.root,
            SIZE,
            vec![
                node(ids.users, u),
                node(ids.alice, al),
                node(ids.windows, w),
            ],
        );
        let (a2, b2) = (r(0., 0., 500., 600.), r(500., 0., 300., 600.));
        let new = lay(ids.alice, SIZE, vec![node(ids.a, a2), node(ids.b, b2)]);
        let mut anim = Animator::new(true);
        let t0 = Instant::now();
        anim.on_rebuild(&t, Some(&old), &new, AREA, t0);

        let a = AnimKey::Node(ids.a);
        let start = map_rect(a2, AREA, al);
        assert_eq!(
            anim.shown(a, a2, t0),
            Shown {
                rect: start,
                alpha: 0.0
            }
        );
        // Ease-in-out: a quarter of the time is 6.25 % of the way.
        assert_close(
            anim.shown(a, a2, t0 + ZOOM / 4),
            Shown {
                rect: lerp_rect(start, a2, 0.0625),
                alpha: 0.0625,
            },
        );
        assert_eq!(
            anim.shown(a, a2, t0 + ZOOM),
            Shown {
                rect: a2,
                alpha: 1.0
            }
        );

        // The whole old treemap grows with the camera while fading out.
        assert_eq!(anim.ghosts(t0).len(), 3);
        assert_eq!(
            ghost_of(&anim, &Item::Node(ids.users), t0),
            Some(Shown {
                rect: u,
                alpha: 1.0
            })
        );
        let half = t0 + ZOOM / 2;
        let gu = ghost_of(&anim, &Item::Node(ids.users), half).expect("fading");
        assert_close(
            gu,
            Shown {
                rect: lerp_rect(u, map_rect(u, al, AREA), 0.5),
                alpha: 0.5,
            },
        );
        assert!(anim.is_animating(half));
        assert!(!anim.is_animating(t0 + ZOOM));
        assert!(anim.ghosts(t0 + ZOOM).is_empty());
    }

    #[test]
    fn zoom_out_and_appear() {
        let (t, ids) = tree();
        let (u, al) = (r(0., 0., 600., 600.), r(3., 19., 594., 578.));
        let inside = lay(ids.alice, SIZE, vec![node(ids.a, r(0., 0., 500., 600.))]);
        let up = lay(
            ids.root,
            SIZE,
            vec![node(ids.users, u), node(ids.alice, al)],
        );
        let mut anim = Animator::new(true);
        let t0 = Instant::now();
        anim.on_rebuild(&t, Some(&inside), &up, AREA, t0);
        // The new treemap comes from inside alice's rectangle…
        assert_eq!(
            anim.shown(AnimKey::Node(ids.users), u, t0),
            Shown {
                rect: map_rect(u, al, AREA),
                alpha: 0.0
            }
        );
        // …while the old one shrinks into it.
        let late = t0 + ZOOM - Duration::from_millis(2);
        let ga = ghost_of(&anim, &Item::Node(ids.a), late).expect("fading");
        let into = map_rect(r(0., 0., 500., 600.), AREA, al);
        assert!(near_eps(ga.rect, into, 1e-2), "{ga:?}");

        // A sibling root: a 200 ms cross-fade in place.
        let users_view = lay(ids.users, SIZE, vec![node(ids.alice, AREA)]);
        let windows_view = lay(ids.windows, SIZE, vec![node(ids.dll, AREA)]);
        let t1 = t0 + ZOOM;
        anim.on_rebuild(&t, Some(&users_view), &windows_view, AREA, t1);
        let dll = AnimKey::Node(ids.dll);
        assert_eq!(
            anim.shown(dll, AREA, t1),
            Shown {
                rect: AREA,
                alpha: 0.0
            }
        );
        assert_close(
            anim.shown(dll, AREA, t1 + APPEAR / 2),
            Shown {
                rect: AREA,
                alpha: 0.875,
            },
        );
        assert_close(
            ghost_of(&anim, &Item::Node(ids.alice), t1 + APPEAR / 2).expect("fading"),
            Shown {
                rect: AREA,
                alpha: 0.125,
            },
        );
        assert!(!anim.is_animating(t1 + APPEAR));
    }

    #[test]
    fn interrupted_transition_is_continuous() {
        let (t, ids) = tree();
        let l1 = lay(
            ids.root,
            SIZE,
            vec![
                node(ids.users, r(0., 0., 400., 600.)),
                node(ids.windows, r(400., 0., 300., 600.)),
                node(ids.bin, r(700., 0., 100., 600.)),
            ],
        );
        let (u2, w2) = (r(0., 0., 600., 600.), r(600., 0., 200., 600.));
        let l2 = lay(
            ids.root,
            SIZE,
            vec![node(ids.users, u2), node(ids.windows, w2)],
        );
        let (u3, al3) = (r(0., 0., 800., 600.), r(2., 17., 796., 581.));
        let l3 = lay(
            ids.root,
            SIZE,
            vec![node(ids.users, u3), node(ids.alice, al3)],
        );
        let mut anim = Animator::new(true);
        let t0 = Instant::now();
        anim.on_rebuild(&t, Some(&l1), &l2, AREA, t0);

        let t1 = t0 + Duration::from_millis(100);
        let users = AnimKey::Node(ids.users);
        let (bin, windows) = (Item::Node(ids.bin), Item::Node(ids.windows));
        let before_u = anim.shown(users, u2, t1);
        let before_w = anim.shown(AnimKey::Node(ids.windows), w2, t1);
        let before_bin = ghost_of(&anim, &bin, t1).expect("bin is fading");
        assert!(before_bin.alpha > 0.0 && before_bin.alpha < 1.0);

        anim.on_rebuild(&t, Some(&l2), &l3, AREA, t1);
        // No jump: kept blocks, new ghosts and carried ghosts start where they were.
        assert_close(anim.shown(users, u3, t1), before_u);
        assert_close(ghost_of(&anim, &windows, t1).expect("new ghost"), before_w);
        assert_close(
            ghost_of(&anim, &bin, t1).expect("carried ghost"),
            before_bin,
        );
        assert!(anim.is_animating(t1));
        // The carried ghost keeps its destination: the centre of its first rectangle.
        let late = t1 + REFLOW - Duration::from_millis(2);
        let gb = ghost_of(&anim, &bin, late).expect("still fading");
        assert!(near_eps(
            gb.rect,
            scale_about_center(r(700., 0., 100., 600.), 0.0),
            1e-2
        ));
        let end = t1 + REFLOW;
        assert_eq!(
            anim.shown(users, u3, end),
            Shown {
                rect: u3,
                alpha: 1.0
            }
        );
        assert!(anim.ghosts(end).is_empty());
    }

    #[test]
    fn snap_drops_everything() {
        let (t, ids) = tree();
        let l1 = lay(
            ids.root,
            SIZE,
            vec![
                node(ids.users, r(0., 0., 400., 600.)),
                node(ids.bin, r(400., 0., 400., 600.)),
            ],
        );
        let l2 = lay(ids.root, SIZE, vec![node(ids.users, AREA)]);
        let mut anim = Animator::new(true);
        let t0 = Instant::now();
        anim.on_rebuild(&t, Some(&l1), &l2, AREA, t0);
        assert!(anim.is_animating(t0));
        assert!(!anim.ghosts(t0).is_empty());

        let t1 = t0 + Duration::from_millis(50);
        let wide = r(0., 0., 1000., 600.);
        let l3 = lay(ids.root, (1000, 600), vec![node(ids.users, wide)]);
        anim.on_rebuild(&t, Some(&l2), &l3, wide, t1);
        assert!(!anim.is_animating(t1));
        assert!(anim.ghosts(t1).is_empty());
        let users = AnimKey::Node(ids.users);
        assert_eq!(
            anim.shown(users, wide, t1),
            Shown {
                rect: wide,
                alpha: 1.0
            }
        );

        // `reset` drops a running transition too.
        anim.on_rebuild(&t, Some(&l1), &l2, AREA, t1);
        assert!(anim.is_animating(t1));
        anim.reset();
        assert!(!anim.is_animating(t1));
        assert!(anim.ghosts(t1).is_empty());
        assert_eq!(
            anim.shown(users, AREA, t1),
            Shown {
                rect: AREA,
                alpha: 1.0
            }
        );
    }

    #[test]
    fn disabled_or_huge_layouts_do_not_animate() {
        let (t, ids) = tree();
        let u1 = r(0., 0., 400., 600.);
        let l1 = lay(
            ids.root,
            SIZE,
            vec![node(ids.users, u1), node(ids.bin, r(400., 0., 400., 600.))],
        );
        let l2 = lay(ids.root, SIZE, vec![node(ids.users, AREA)]);
        let t0 = Instant::now();
        let users = AnimKey::Node(ids.users);

        let mut off = Animator::new(false);
        off.on_rebuild(&t, None, &l1, AREA, t0);
        assert!(!off.is_animating(t0));
        assert_eq!(
            off.shown(users, u1, t0),
            Shown {
                rect: u1,
                alpha: 1.0
            }
        );
        off.on_rebuild(&t, Some(&l1), &l2, AREA, t0);
        assert!(!off.is_animating(t0));
        assert!(off.ghosts(t0).is_empty());
        assert_eq!(
            off.shown(users, AREA, t0),
            Shown {
                rect: AREA,
                alpha: 1.0
            }
        );

        // Blocks never looked up in the tree: only keys of the new layout.
        let many = |n: usize| {
            let items = (0..n)
                .map(|i| node(NodeId(1000 + i as u32), r(i as f32 % 800., 0., 1., 1.)))
                .collect();
            lay(ids.root, SIZE, items)
        };
        let mut on = Animator::new(true);
        on.on_rebuild(&t, None, &many(MAX_ANIMATED), AREA, t0);
        assert!(on.is_animating(t0), "the limit itself still animates");
        on.on_rebuild(&t, None, &many(MAX_ANIMATED + 1), AREA, t0);
        assert!(!on.is_animating(t0));

        // Ghosts count too: users and the bin vanish.
        on.on_rebuild(&t, Some(&l1), &many(MAX_ANIMATED - 2), AREA, t0);
        assert!(on.is_animating(t0), "5998 blocks + 2 ghosts");
        assert_eq!(on.ghosts(t0).len(), 2);
        on.reset();
        on.on_rebuild(&t, Some(&l1), &many(MAX_ANIMATED - 1), AREA, t0);
        assert!(!on.is_animating(t0), "5999 blocks + 2 ghosts");
        assert!(on.ghosts(t0).is_empty());
    }

    #[test]
    fn hover_fade_rises_and_falls() {
        let key = AnimKey::Node(NodeId(7));
        let mut h = HoverFade::new(true);
        assert_eq!(h.level(key), 0.0);
        h.update(Some(key), 0.06);
        assert!((h.level(key) - 0.5).abs() < 1e-3);
        assert!(h.is_animating());
        h.update(Some(key), 0.06);
        assert!((h.level(key) - 1.0).abs() < 1e-3);
        assert!(!h.is_animating(), "settled at 1");
        h.update(None, 0.06);
        assert!((h.level(key) - 0.5).abs() < 1e-3);
        assert!(h.is_animating());
        h.update(None, 0.06);
        assert_eq!(h.level(key), 0.0);
        assert!(!h.is_animating());
    }

    #[test]
    fn hover_fade_off_is_instant() {
        let key = AnimKey::Node(NodeId(7));
        let mut h = HoverFade::new(false);
        h.update(Some(key), 0.001);
        assert_eq!(h.level(key), 1.0);
        h.update(None, 0.001);
        assert_eq!(h.level(key), 0.0);
        assert!(!h.is_animating());
    }

    #[test]
    fn glide_moves_between_selections() {
        let (k1, k2) = (AnimKey::Node(NodeId(1)), AnimKey::Node(NodeId(2)));
        let (r1, r2) = (r(0., 0., 10., 10.), r(100., 0., 20., 10.));
        let mut g = Glide::new(true);
        let t0 = Instant::now();
        assert_eq!(g.rect(Some(k1), r1, t0), r1);
        assert!(!g.is_animating(t0));
        // New key: starts on the old rectangle, ends on the new target.
        assert_eq!(g.rect(Some(k2), r2, t0), r1);
        assert!(g.is_animating(t0));
        let mid = g.rect(Some(k2), r2, t0 + GLIDE / 2);
        assert!(mid.x > 0. && mid.x < 100.);
        assert_eq!(g.rect(Some(k2), r2, t0 + GLIDE), r2);
        assert!(!g.is_animating(t0 + GLIDE));
        // Same key, moving target (a reflow): follows it without delay.
        let r3 = r(300., 0., 20., 10.);
        assert_eq!(g.rect(Some(k2), r3, t0 + GLIDE * 2), r3);
        assert!(!g.is_animating(t0 + GLIDE * 2));
    }

    #[test]
    fn glide_off_returns_the_target() {
        let (k1, k2) = (AnimKey::Node(NodeId(1)), AnimKey::Node(NodeId(2)));
        let mut g = Glide::new(false);
        let t0 = Instant::now();
        assert_eq!(g.rect(Some(k1), r(0., 0., 1., 1.), t0), r(0., 0., 1., 1.));
        assert_eq!(g.rect(Some(k2), r(5., 5., 1., 1.), t0), r(5., 5., 1., 1.));
        assert!(!g.is_animating(t0));
    }
}

use crate::tree::{NodeId, NodeKind, Tree};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    pub fn area(&self) -> f32 {
        self.w * self.h
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Node(NodeId),
    SmallGroup {
        parent: NodeId,
        files: u64,
        size: u64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub item: Item,
    pub rect: Rect,
    pub depth: u16,
    pub header: bool,
}

pub struct LayoutParams {
    pub gap: f32,
    pub pad: f32,
    pub header_h: f32,
    pub header_min_w: f32,
    pub header_min_h: f32,
    pub min_area: f32,
    pub min_side: f32,
}

impl Default for LayoutParams {
    fn default() -> Self {
        LayoutParams {
            gap: 1.5,
            pad: 3.0,
            header_h: 16.0,
            header_min_w: 64.0,
            header_min_h: 40.0,
            min_area: 48.0,
            min_side: 2.0,
        }
    }
}

/// Worst aspect ratio of a row of `areas` laid along a side of length `side`.
fn worst(areas: &[f32], side: f32) -> f32 {
    let sum: f32 = areas.iter().sum();
    let max = areas.iter().copied().fold(f32::MIN, f32::max);
    let min = areas.iter().copied().fold(f32::MAX, f32::min);
    let s2 = side * side;
    ((s2 * max) / (sum * sum)).max((sum * sum) / (s2 * min))
}

/// Squarified treemap (Bruls et al.). `sizes` sorted descending and > 0;
/// the output has the same order.
pub fn squarify(sizes: &[u64], bounds: Rect) -> Vec<Rect> {
    let total: f64 = sizes.iter().map(|&s| s as f64).sum();
    if sizes.is_empty() || total <= 0.0 || bounds.w <= 0.0 || bounds.h <= 0.0 {
        return vec![
            Rect {
                x: bounds.x,
                y: bounds.y,
                w: 0.0,
                h: 0.0
            };
            sizes.len()
        ];
    }
    let scale = f64::from(bounds.area()) / total;
    let areas: Vec<f32> = sizes.iter().map(|&s| (s as f64 * scale) as f32).collect();
    let mut out = Vec::with_capacity(sizes.len());
    let mut rest = bounds;
    let mut i = 0;
    while i < areas.len() {
        let side = rest.w.min(rest.h);
        let mut end = i + 1;
        while end < areas.len() && worst(&areas[i..=end], side) <= worst(&areas[i..end], side) {
            end += 1;
        }
        let row = &areas[i..end];
        let sum: f32 = row.iter().sum();
        if rest.w >= rest.h {
            // Column on the left, items stacked vertically.
            let cw = if i + row.len() == areas.len() {
                rest.w
            } else {
                (sum / rest.h).min(rest.w)
            };
            let mut y = rest.y;
            for (k, a) in row.iter().enumerate() {
                let h = if k + 1 == row.len() {
                    rest.y + rest.h - y
                } else {
                    a / cw
                };
                out.push(Rect {
                    x: rest.x,
                    y,
                    w: cw,
                    h,
                });
                y += h;
            }
            rest.x += cw;
            rest.w -= cw;
        } else {
            // Row on top, items side by side.
            let rh = if i + row.len() == areas.len() {
                rest.h
            } else {
                (sum / rest.w).min(rest.h)
            };
            let mut x = rest.x;
            for (k, a) in row.iter().enumerate() {
                let w = if k + 1 == row.len() {
                    rest.x + rest.w - x
                } else {
                    a / rh
                };
                out.push(Rect {
                    x,
                    y: rest.y,
                    w,
                    h: rh,
                });
                x += w;
            }
            rest.y += rh;
            rest.h -= rh;
        }
        i = end;
    }
    out
}

/// Nested squarified layout of `root`'s subtree inside `bounds`. The root
/// itself is not placed; parents come before their children.
pub fn layout(tree: &Tree, root: NodeId, bounds: Rect, p: &LayoutParams) -> Vec<Placed> {
    let mut out = Vec::new();
    place_children(tree, root, bounds, 0, p, &mut out);
    out
}

fn place_children(
    tree: &Tree,
    parent: NodeId,
    area: Rect,
    depth: u16,
    p: &LayoutParams,
    out: &mut Vec<Placed>,
) {
    let mut items: Vec<(Item, u64)> = tree
        .node(parent)
        .children
        .iter()
        .map(|&c| (c, tree.node(c).size))
        .filter(|&(_, s)| s > 0)
        .map(|(c, s)| (Item::Node(c), s))
        .collect();
    if items.is_empty() {
        return;
    }
    let total: u64 = items.iter().map(|(_, s)| s).sum();
    let projected = |s: u64| (s as f64 / total as f64 * f64::from(area.area())) as f32;
    let small = items
        .iter()
        .filter(|(_, s)| projected(*s) < p.min_area)
        .count();
    if small >= 2 {
        let (mut files, mut size) = (0u64, 0u64);
        items.retain(|(item, s)| {
            if projected(*s) < p.min_area {
                if let Item::Node(id) = item {
                    files += tree.node(*id).files;
                }
                size += s;
                false
            } else {
                true
            }
        });
        items.push((
            Item::SmallGroup {
                parent,
                files,
                size,
            },
            size,
        ));
    }
    items.sort_by_key(|i| std::cmp::Reverse(i.1));
    let sizes: Vec<u64> = items.iter().map(|(_, s)| *s).collect();
    let rects = squarify(&sizes, area);
    for ((item, _), r) in items.into_iter().zip(rects) {
        let r = Rect {
            x: r.x + p.gap / 2.0,
            y: r.y + p.gap / 2.0,
            w: r.w - p.gap,
            h: r.h - p.gap,
        };
        if r.w <= 0.0 || r.h <= 0.0 {
            continue;
        }
        let dir = match item {
            Item::Node(id) if tree.node(id).kind == NodeKind::Dir => Some(id),
            _ => None,
        };
        let header = dir.is_some() && r.w >= p.header_min_w && r.h >= p.header_min_h;
        out.push(Placed {
            item,
            rect: r,
            depth,
            header,
        });
        if let Some(id) = dir {
            let content = if header {
                Rect {
                    x: r.x + p.pad,
                    y: r.y + p.header_h,
                    w: r.w - 2.0 * p.pad,
                    h: r.h - p.header_h - p.pad,
                }
            } else {
                Rect {
                    x: r.x + p.pad,
                    y: r.y + p.pad,
                    w: r.w - 2.0 * p.pad,
                    h: r.h - 2.0 * p.pad,
                }
            };
            if header
                && content.w >= p.min_side
                && content.h >= p.min_side
                && content.area() >= p.min_area
            {
                place_children(tree, id, content, depth + 1, p, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::EntryData;
    use std::path::PathBuf;

    fn approx(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol * b.max(1.0)
    }
    fn entry(n: &str, kind: NodeKind, size: u64) -> EntryData {
        EntryData {
            name: n.into(),
            kind,
            size,
            newest: 0,
        }
    }
    fn new_tree() -> Tree {
        Tree::new(PathBuf::from(r"C:\"), 0)
    }
    // C:\ { A(dir){ a1 40, a2 20 }, B 40 }
    fn nested() -> (Tree, NodeId, NodeId, NodeId) {
        let mut t = new_tree();
        let root = t.root();
        let ids = t.add_children(
            root,
            vec![entry("A", NodeKind::Dir, 0), entry("B", NodeKind::File, 40)],
        );
        let (a, b) = (ids[0], ids[1]);
        let kids = t.add_children(
            a,
            vec![
                entry("a1", NodeKind::File, 40),
                entry("a2", NodeKind::File, 20),
            ],
        );
        (t, a, kids[0], b)
    }

    #[test]
    fn squarify_areas_are_proportional_and_cover_bounds() {
        let sizes = [6, 6, 4, 3, 2, 2, 1];
        let b = Rect {
            x: 0.,
            y: 0.,
            w: 600.,
            h: 400.,
        };
        let rs = squarify(&sizes, b);
        let total: f32 = rs.iter().map(|r| r.area()).sum();
        assert!(approx(total, b.area(), 0.005));
        for (r, s) in rs.iter().zip(sizes) {
            assert!(approx(r.area(), b.area() * s as f32 / 24.0, 0.005));
        }
        for r in &rs {
            assert!(r.x >= -0.01 && r.y >= -0.01 && r.x + r.w <= 600.01 && r.y + r.h <= 400.01);
        }
        for (i, a) in rs.iter().enumerate() {
            for c in &rs[i + 1..] {
                let ox = (a.x + a.w).min(c.x + c.w) - a.x.max(c.x);
                let oy = (a.y + a.h).min(c.y + c.h) - a.y.max(c.y);
                assert!(ox <= 0.01 || oy <= 0.01, "chevauchement");
            }
        }
        assert!(rs.iter().all(|r| r.w.max(r.h) / r.w.min(r.h) <= 4.0));
    }

    #[test]
    fn nested_layout_has_headers_and_parent_first_order() {
        let (t, a, a1, b) = nested();
        let placed = layout(
            &t,
            t.root(),
            Rect {
                x: 0.,
                y: 0.,
                w: 400.,
                h: 300.,
            },
            &LayoutParams::default(),
        );
        let ia = placed.iter().position(|p| p.item == Item::Node(a)).unwrap();
        let ia1 = placed
            .iter()
            .position(|p| p.item == Item::Node(a1))
            .unwrap();
        assert!(placed[ia].header && ia < ia1 && placed[ia1].depth == 1);
        let (pa, p1) = (placed[ia].rect, placed[ia1].rect);
        assert!(p1.y >= pa.y + 16.0 - 0.01 && p1.x >= pa.x && p1.x + p1.w <= pa.x + pa.w + 0.01);
        assert!(
            placed
                .iter()
                .any(|p| p.item == Item::Node(b) && p.depth == 0)
        );
    }

    #[test]
    fn tiny_children_are_grouped() {
        let mut t = new_tree();
        let root = t.root();
        let mut entries = vec![entry("big", NodeKind::File, 100_000)];
        for i in 0..50 {
            entries.push(entry(&format!("s{i}"), NodeKind::File, 30));
        }
        t.add_children(root, entries);
        let placed = layout(
            &t,
            t.root(),
            Rect {
                x: 0.,
                y: 0.,
                w: 400.,
                h: 300.,
            },
            &LayoutParams::default(),
        );
        assert!(placed.iter().any(|p| p.item
            == Item::SmallGroup {
                parent: t.root(),
                files: 50,
                size: 1500
            }));
        assert_eq!(placed.len(), 2);
    }

    #[test]
    fn small_dir_has_no_header() {
        let (t, ..) = nested();
        let placed = layout(
            &t,
            t.root(),
            Rect {
                x: 0.,
                y: 0.,
                w: 40.,
                h: 30.,
            },
            &LayoutParams::default(),
        );
        assert!(!placed.is_empty());
        assert!(placed.iter().all(|p| !p.header));
    }

    #[test]
    fn zero_size_and_empty_are_skipped() {
        let b = Rect {
            x: 0.,
            y: 0.,
            w: 400.,
            h: 300.,
        };
        let t = new_tree();
        assert!(layout(&t, t.root(), b, &LayoutParams::default()).is_empty());
        let mut t = new_tree();
        let root = t.root();
        let ids = t.add_children(
            root,
            vec![
                entry("z", NodeKind::File, 0),
                entry("f", NodeKind::File, 10),
            ],
        );
        let placed = layout(&t, t.root(), b, &LayoutParams::default());
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].item, Item::Node(ids[1]));
    }

    #[test]
    fn small_dir_is_closed() {
        // A (dir, 60) and B (40) in 120x80: A is about 72x80 -> header; shrink to close it.
        let (t, a, ..) = nested();
        let b = Rect {
            x: 0.,
            y: 0.,
            w: 100.,
            h: 38.,
        };
        let placed = layout(&t, t.root(), b, &LayoutParams::default());
        let pa = placed.iter().find(|p| p.item == Item::Node(a)).unwrap();
        assert!(!pa.header);
        assert_eq!(placed.len(), 2, "no child of a closed dir is placed");
        assert!(placed.iter().all(|p| p.depth == 0));
    }
}

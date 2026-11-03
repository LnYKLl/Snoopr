//! Treemap model: the layout cache, its rebuild policy, hit testing and tones.

use crate::theme::{HUES, Tone};
use snoopr_core::actions::verdict;
use snoopr_core::layout::{Item, LayoutParams, Placed, Rect, layout};
use snoopr_core::rules::{RuleContext, Verdict};
use snoopr_core::tree::{NodeId, NodeKind, Tree};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// While a scan runs, the layout is rebuilt at most this often.
const SCAN_REBUILD_EVERY: Duration = Duration::from_millis(250);

/// What a cached layout was built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutKey {
    pub generation: u64,
    pub view_root: NodeId,
    pub size: (u32, u32),
}

/// One placed block with everything painting needs, so painting never reads the tree.
#[derive(Clone, Debug)]
pub struct CachedItem {
    pub placed: Placed,
    pub tone: Tone,
    /// Empty when the displayed name is localized and built at paint time:
    /// small-files group, Recycle Bin, "Not scanned".
    pub name: String,
    pub size: u64,
    pub newest: i64,
    pub files: u64,
    /// `Dir` blocks are drawn as containers; a small-files group is a `File`.
    pub kind: NodeKind,
}

pub struct CachedLayout {
    pub key: LayoutKey,
    pub built_at: Instant,
    /// In layout order: parents before their children.
    pub items: Vec<CachedItem>,
}

pub fn needs_rebuild(
    prev: Option<&CachedLayout>,
    key: &LayoutKey,
    now: Instant,
    scanning: bool,
) -> bool {
    let Some(prev) = prev else { return true };
    if prev.key == *key {
        return false;
    }
    if prev.key.view_root != key.view_root || prev.key.size != key.size || !scanning {
        return true;
    }
    now.saturating_duration_since(prev.built_at) >= SCAN_REBUILD_EVERY
}

/// The root's children sorted by decreasing size (ties by id) get `HUES[i % 8]`.
pub fn hue_map(tree: &Tree) -> HashMap<NodeId, f32> {
    let mut kids = tree.node(tree.root()).children.clone();
    kids.sort_by_key(|&c| (std::cmp::Reverse(tree.node(c).size), c));
    kids.into_iter()
        .enumerate()
        .map(|(i, c)| (c, HUES[i % HUES.len()]))
        .collect()
}

/// Tone rule, in order: Unscanned, Recycle Bin (or inside it), system block, top-level hue.
fn tone_rule(
    tree: &Tree,
    rules: &RuleContext,
    hues: &HashMap<NodeId, f32>,
    bin: Option<NodeId>,
    id: NodeId,
) -> Tone {
    if tree.node(id).kind == NodeKind::Unscanned {
        return Tone::Unscanned;
    }
    if bin.is_some_and(|b| b == id || tree.is_ancestor(b, id)) {
        return Tone::RecycleBin;
    }
    if let Verdict::Block(b) = verdict(tree, rules, id)
        && b.is_system()
    {
        return Tone::System;
    }
    let hue = tree
        .top_level_of(id)
        .and_then(|t| hues.get(&t).copied())
        .unwrap_or(HUES[0]);
    Tone::Normal(hue)
}

/// Lays out `key.view_root` in `bounds` and precomputes tone, name and figures of
/// every block. Takes a read view of the tree; call it under one `tree.read()`.
pub fn build_layout(
    tree: &Tree,
    rules: &RuleContext,
    key: LayoutKey,
    bounds: Rect,
    now: Instant,
) -> CachedLayout {
    let placed = layout(tree, key.view_root, bounds, &LayoutParams::default());
    let hues = hue_map(tree);
    let bin = tree.recycle_bin();
    // Tones of directories, reused by their children: a system block or the
    // Recycle Bin always covers the whole subtree, so `verdict` is skipped there.
    let mut dir_tones: HashMap<NodeId, Tone> = HashMap::new();
    let mut tone_for = |id: NodeId| -> Tone {
        if let Some(&t) = dir_tones.get(&id) {
            return t;
        }
        let inherited = tree
            .node(id)
            .parent
            .and_then(|p| dir_tones.get(&p).copied())
            .filter(|t| matches!(t, Tone::System | Tone::RecycleBin));
        let t = inherited.unwrap_or_else(|| tone_rule(tree, rules, &hues, bin, id));
        if tree.node(id).kind == NodeKind::Dir {
            dir_tones.insert(id, t);
        }
        t
    };
    let shown: HashSet<NodeId> = placed
        .iter()
        .filter_map(|p| match p.item {
            Item::Node(id) => Some(id),
            Item::SmallGroup { .. } => None,
        })
        .collect();
    let items = placed
        .into_iter()
        .map(|p| match p.item {
            Item::Node(id) => {
                let n = tree.node(id);
                let localized = n.kind == NodeKind::Unscanned || Some(id) == bin;
                CachedItem {
                    tone: tone_for(id),
                    name: if localized {
                        String::new()
                    } else {
                        n.name.to_string_lossy().into_owned()
                    },
                    size: n.size,
                    newest: n.newest,
                    files: n.files,
                    kind: n.kind,
                    placed: p,
                }
            }
            Item::SmallGroup {
                parent,
                files,
                size,
            } => {
                // Newest of the grouped children: the parent's non-empty children not placed.
                let pn = tree.node(parent);
                let newest = pn
                    .children
                    .iter()
                    .filter(|c| !shown.contains(c))
                    .map(|&c| tree.node(c))
                    .filter(|n| n.size > 0)
                    .map(|n| n.newest)
                    .max()
                    .unwrap_or(pn.newest);
                CachedItem {
                    tone: tone_for(parent),
                    name: String::new(),
                    size,
                    newest,
                    files,
                    kind: NodeKind::File,
                    placed: p,
                }
            }
        })
        .collect();
    CachedLayout {
        key,
        built_at: now,
        items,
    }
}

/// Index of the last (deepest) item containing the point.
pub fn hit_test(items: &[CachedItem], x: f32, y: f32) -> Option<usize> {
    items.iter().rposition(|it| it.placed.rect.contains(x, y))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelMode {
    NameAndSize,
    NameOnly,
    Hidden,
}

pub fn label_mode(w: f32, h: f32) -> LabelMode {
    if w >= 58.0 && h >= 32.0 {
        LabelMode::NameAndSize
    } else if w >= 42.0 && h >= 17.0 {
        LabelMode::NameOnly
    } else {
        LabelMode::Hidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snoopr_core::tree::EntryData;
    use std::path::PathBuf;

    fn key(generation: u64, root: u32, size: (u32, u32)) -> LayoutKey {
        LayoutKey {
            generation,
            view_root: NodeId(root),
            size,
        }
    }

    fn cached(k: LayoutKey, built_at: Instant) -> CachedLayout {
        CachedLayout {
            key: k,
            built_at,
            items: Vec::new(),
        }
    }

    #[test]
    fn needs_rebuild_table() {
        let t0 = Instant::now();
        let k = key(7, 0, (800, 600));
        let prev = cached(k, t0);
        let soon = t0 + Duration::from_millis(100);
        let later = t0 + Duration::from_millis(300);
        assert!(needs_rebuild(None, &k, t0, false));
        assert!(!needs_rebuild(Some(&prev), &k, soon, false));
        assert!(!needs_rebuild(Some(&prev), &k, later, true));
        assert!(needs_rebuild(
            Some(&prev),
            &key(7, 3, (800, 600)),
            soon,
            true
        ));
        assert!(needs_rebuild(
            Some(&prev),
            &key(7, 0, (801, 600)),
            soon,
            true
        ));
        assert!(needs_rebuild(
            Some(&prev),
            &key(8, 0, (800, 600)),
            soon,
            false
        ));
        assert!(!needs_rebuild(
            Some(&prev),
            &key(8, 0, (800, 600)),
            soon,
            true
        ));
        assert!(needs_rebuild(
            Some(&prev),
            &key(8, 0, (800, 600)),
            later,
            true
        ));
    }

    fn item(id: u32, x: f32, y: f32, w: f32, h: f32) -> CachedItem {
        CachedItem {
            placed: Placed {
                item: Item::Node(NodeId(id)),
                rect: Rect { x, y, w, h },
                depth: 0,
                header: true,
            },
            tone: Tone::Normal(HUES[0]),
            name: format!("n{id}"),
            size: 1,
            newest: 0,
            files: 1,
            kind: NodeKind::Dir,
        }
    }

    #[test]
    fn hit_test_prefers_deepest() {
        let items = [item(1, 0., 0., 100., 100.), item(2, 10., 20., 30., 30.)];
        assert_eq!(hit_test(&items, 15., 25.), Some(1));
        assert_eq!(hit_test(&items, 90., 90.), Some(0));
        assert_eq!(hit_test(&items, 200., 200.), None);
    }

    #[test]
    fn label_thresholds() {
        assert_eq!(label_mode(58., 32.), LabelMode::NameAndSize);
        assert_eq!(label_mode(57., 32.), LabelMode::NameOnly);
        assert_eq!(label_mode(42., 17.), LabelMode::NameOnly);
        assert_eq!(label_mode(41., 17.), LabelMode::Hidden);
    }

    fn ctx() -> RuleContext {
        RuleContext {
            windows_dir: r"C:\Windows".into(),
            program_data: r"C:\ProgramData".into(),
            users_dir: r"C:\Users".into(),
            cloud_roots: Vec::new(),
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

    const BOUNDS: Rect = Rect {
        x: 0.,
        y: 0.,
        w: 800.,
        h: 600.,
    };

    fn tone_of(l: &CachedLayout, id: NodeId) -> Tone {
        l.items
            .iter()
            .find(|it| it.placed.item == Item::Node(id))
            .map(|it| it.tone)
            .expect("item is placed")
    }

    #[test]
    fn tones() {
        let mut t = Tree::new(PathBuf::from(r"C:\"), 0);
        let root = t.root();
        let top = t.add_children(
            root,
            vec![
                e("Windows", NodeKind::Dir, 0),
                e("Users", NodeKind::Dir, 0),
                e("$Recycle.Bin", NodeKind::Dir, 0),
            ],
        );
        let (windows, users, bin) = (top[0], top[1], top[2]);
        t.add_children(windows, vec![e("x", NodeKind::File, 100)]);
        let y = t.add_children(users, vec![e("y", NodeKind::File, 100)])[0];
        let z = t.add_children(bin, vec![e("z", NodeKind::File, 100)])[0];
        t.set_unscanned(5);
        let unscanned = t
            .node(root)
            .children
            .iter()
            .copied()
            .find(|&c| t.node(c).kind == NodeKind::Unscanned)
            .unwrap();
        let k = LayoutKey {
            generation: t.generation(),
            view_root: root,
            size: (800, 600),
        };
        let l = build_layout(&t, &ctx(), k, BOUNDS, Instant::now());
        assert_eq!(l.key, k);
        assert_eq!(tone_of(&l, windows), Tone::System);
        assert!(matches!(tone_of(&l, y), Tone::Normal(_)));
        assert_eq!(tone_of(&l, z), Tone::RecycleBin);
        assert_eq!(tone_of(&l, unscanned), Tone::Unscanned);
    }

    #[test]
    fn small_group_takes_parent_tone_and_localized_names_are_empty() {
        let mut t = Tree::new(PathBuf::from(r"C:\"), 0);
        let root = t.root();
        let top = t.add_children(
            root,
            vec![
                e("Windows", NodeKind::Dir, 0),
                e("$Recycle.Bin", NodeKind::Dir, 0),
            ],
        );
        let (windows, bin) = (top[0], top[1]);
        // 500 files of 30 are each far below the minimum area, together ~4 % of Windows.
        let mut kids = vec![e("big", NodeKind::File, 360_000)];
        for i in 0..500 {
            kids.push(EntryData {
                newest: 40 + i,
                ..e(&format!("s{i}"), NodeKind::File, 30)
            });
        }
        t.add_children(windows, kids);
        t.add_children(bin, vec![e("z", NodeKind::File, 100_000)]);
        let k = LayoutKey {
            generation: t.generation(),
            view_root: root,
            size: (800, 600),
        };
        let l = build_layout(&t, &ctx(), k, BOUNDS, Instant::now());
        let group = l
            .items
            .iter()
            .find(
                |it| matches!(it.placed.item, Item::SmallGroup { parent, .. } if parent == windows),
            )
            .expect("small files are grouped");
        assert_eq!(group.tone, Tone::System);
        assert_eq!((group.files, group.size, group.newest), (500, 15_000, 539));
        assert!(group.name.is_empty());
        let bin_item = l
            .items
            .iter()
            .find(|it| it.placed.item == Item::Node(bin))
            .unwrap();
        assert!(bin_item.name.is_empty());
        let w = l
            .items
            .iter()
            .find(|it| it.placed.item == Item::Node(windows))
            .unwrap();
        assert_eq!(w.name, "Windows");
    }

    #[test]
    fn hue_map_by_size() {
        let mut t = Tree::new(PathBuf::from(r"D:\"), 0);
        let root = t.root();
        let ids = t.add_children(
            root,
            vec![
                e("a", NodeKind::File, 10),
                e("b", NodeKind::File, 30),
                e("c", NodeKind::File, 20),
            ],
        );
        let h = hue_map(&t);
        assert_eq!(h[&ids[1]], HUES[0]);
        assert_eq!(h[&ids[2]], HUES[1]);
        assert_eq!(h[&ids[0]], HUES[2]);
    }
}

//! Treemap model: the layout cache, its rebuild policy, hit testing and tones.

use crate::theme::{Container, Tone};
use snoopr_core::filetype::FileType;
use snoopr_core::layout::{Item, LayoutParams, Placed, Rect, layout};
use snoopr_core::tree::{NodeId, NodeKind, Tree};
use std::collections::HashSet;
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
    /// Fill of solid blocks: a file's type, a small-files group's or a closed
    /// folder's dominant type. An open folder keeps its dominant type here too.
    pub tone: Tone,
    /// Look of an open folder; also set, unused, on the other blocks.
    pub container: Container,
    /// A `Dir`'s heaviest type and its share (0..=1), for the tooltip.
    pub dominant: Option<(FileType, f32)>,
    /// Empty when the displayed name is localized and built at paint time:
    /// small-files group, Recycle Bin, "Not scanned".
    pub name: String,
    pub size: u64,
    pub newest: i64,
    pub files: u64,
    /// A `Dir` with `placed.header` is drawn as a container; without it, as a
    /// closed solid block. A small-files group is a `File`.
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

/// Tone of a placed node: Unscanned, the type of a file, else the dominant
/// type of a folder (`Other` when nothing below it is typed).
fn tone_of(tree: &Tree, id: NodeId) -> Tone {
    if tree.node(id).kind == NodeKind::Unscanned {
        return Tone::Unscanned;
    }
    let t = tree
        .file_type(id)
        .or_else(|| tree.dominant_type(id).map(|(t, _)| t));
    Tone::Type(t.unwrap_or(FileType::Other))
}

/// Lays out `key.view_root` in `bounds` and precomputes tone, container, name
/// and figures of every block. Takes a read view of the tree; call it under one
/// `tree.read()`.
pub fn build_layout(tree: &Tree, key: LayoutKey, bounds: Rect, now: Instant) -> CachedLayout {
    let placed = layout(tree, key.view_root, bounds, &LayoutParams::default());
    let bin = tree.recycle_bin();
    let container_of = |id: NodeId| match bin {
        Some(b) if b == id || tree.is_ancestor(b, id) => Container::RecycleBin,
        _ => Container::Normal,
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
                    tone: tone_of(tree, id),
                    container: container_of(id),
                    dominant: tree.dominant_type(id),
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
                    tone: tone_of(tree, parent),
                    container: container_of(parent),
                    dominant: None,
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
            tone: Tone::Type(FileType::Other),
            container: Container::Normal,
            dominant: None,
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

    fn build(t: &Tree) -> CachedLayout {
        let k = LayoutKey {
            generation: t.generation(),
            view_root: t.root(),
            size: (800, 600),
        };
        let l = build_layout(t, k, BOUNDS, Instant::now());
        assert_eq!(l.key, k);
        l
    }

    fn placed(l: &CachedLayout, id: NodeId) -> &CachedItem {
        l.items
            .iter()
            .find(|it| it.placed.item == Item::Node(id))
            .expect("item is placed")
    }

    #[test]
    fn tones_by_type() {
        let mut t = Tree::new(PathBuf::from(r"C:\"), 0);
        let root = t.root();
        let top = t.add_children(
            root,
            vec![e("Users", NodeKind::Dir, 0), e("Windows", NodeKind::Dir, 0)],
        );
        let (users, windows) = (top[0], top[1]);
        let a = t.add_children(users, vec![e("a.mkv", NodeKind::File, 100)])[0];
        let c = t.add_children(windows, vec![e("c.dll", NodeKind::File, 30)])[0];
        t.set_unscanned(5);
        let unscanned = t
            .node(root)
            .children
            .iter()
            .copied()
            .find(|&n| t.node(n).kind == NodeKind::Unscanned)
            .unwrap();
        let l = build(&t);
        assert_eq!(placed(&l, a).tone, Tone::Type(FileType::Media));
        assert_eq!(placed(&l, c).tone, Tone::Type(FileType::System));
        assert_eq!(placed(&l, unscanned).tone, Tone::Unscanned);
        // Open folders: neutral container, dominant type kept for the tooltip.
        let w = placed(&l, windows);
        assert!(w.placed.header);
        assert_eq!(w.container, Container::Normal);
        assert_eq!(w.tone, Tone::Type(FileType::System));
        assert_eq!(w.dominant, Some((FileType::System, 1.0)));
        assert_eq!(placed(&l, a).dominant, None);
    }

    #[test]
    fn closed_dir_uses_dominant() {
        let mut t = Tree::new(PathBuf::from(r"D:\"), 0);
        let root = t.root();
        let top = t.add_children(
            root,
            vec![
                e("big.bin", NodeKind::File, 1_000_000),
                e("d", NodeKind::Dir, 0),
            ],
        );
        let d = top[1];
        let kids = t.add_children(
            d,
            vec![
                e("a.zip", NodeKind::File, 2_000),
                e("b.txt", NodeKind::File, 1_000),
            ],
        );
        let l = build(&t);
        let it = placed(&l, d);
        assert_eq!(it.kind, NodeKind::Dir);
        assert!(!it.placed.header, "too small to be opened");
        assert_eq!(it.tone, Tone::Type(FileType::Archive));
        let (ty, share) = it.dominant.expect("typed bytes");
        assert_eq!(ty, FileType::Archive);
        assert!((share - 2.0 / 3.0).abs() < 1e-6);
        // A closed folder has no placed children.
        assert!(
            l.items
                .iter()
                .all(|it| !kids.iter().any(|&k| it.placed.item == Item::Node(k)))
        );
    }

    #[test]
    fn recycle_container() {
        let mut t = Tree::new(PathBuf::from(r"C:\"), 0);
        let root = t.root();
        let top = t.add_children(
            root,
            vec![
                e("$Recycle.Bin", NodeKind::Dir, 0),
                e("Users", NodeKind::Dir, 0),
            ],
        );
        let (bin, users) = (top[0], top[1]);
        let sid = t.add_children(bin, vec![e("S-1-5-21", NodeKind::Dir, 0)])[0];
        let x = t.add_children(sid, vec![e("x.zip", NodeKind::File, 100)])[0];
        let y = t.add_children(users, vec![e("y.txt", NodeKind::File, 100)])[0];
        let l = build(&t);
        assert_eq!(placed(&l, bin).container, Container::RecycleBin);
        let s = placed(&l, sid);
        assert!(s.placed.header);
        assert_eq!(s.container, Container::RecycleBin);
        assert_eq!(placed(&l, users).container, Container::Normal);
        // Files in the bin keep the colour of their type.
        assert_eq!(placed(&l, x).tone, Tone::Type(FileType::Archive));
        assert_eq!(placed(&l, y).tone, Tone::Type(FileType::Other));
    }

    #[test]
    fn small_group_takes_parent_tone_and_localized_names_are_empty() {
        let mut t = Tree::new(PathBuf::from(r"C:\"), 0);
        let root = t.root();
        let top = t.add_children(
            root,
            vec![
                e("Downloads", NodeKind::Dir, 0),
                e("$Recycle.Bin", NodeKind::Dir, 0),
            ],
        );
        let (downloads, bin) = (top[0], top[1]);
        // 500 files of 30 are each far below the minimum area, together ~4 % of
        // Downloads. They are `Other`; the folder is mostly `Archive`.
        let mut kids = vec![e("big.zip", NodeKind::File, 360_000)];
        for i in 0..500 {
            kids.push(EntryData {
                newest: 40 + i,
                ..e(&format!("s{i}.txt"), NodeKind::File, 30)
            });
        }
        t.add_children(downloads, kids);
        t.add_children(bin, vec![e("z", NodeKind::File, 100_000)]);
        let l = build(&t);
        let group = l
            .items
            .iter()
            .find(
                |it| matches!(it.placed.item, Item::SmallGroup { parent, .. } if parent == downloads),
            )
            .expect("small files are grouped");
        assert_eq!(group.tone, Tone::Type(FileType::Archive));
        assert_eq!(group.kind, NodeKind::File);
        assert_eq!((group.files, group.size, group.newest), (500, 15_000, 539));
        assert!(group.name.is_empty());
        assert!(placed(&l, bin).name.is_empty());
        assert_eq!(placed(&l, downloads).name, "Downloads");
    }
}

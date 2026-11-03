use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::filetype::{Context, FileType, child_context, file_type};

const UNSCANNED_NAME: &str = "<unscanned>";
const RECYCLE_BIN_NAME: &str = "$Recycle.Bin";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Dir,
    File,
    Unscanned,
}

#[derive(Debug)]
pub struct Node {
    pub name: Box<OsStr>,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub kind: NodeKind,
    /// Allocated bytes; for a `Dir`, the sum of its attached descendants.
    pub size: u64,
    /// `File` = 1; `Dir` = number of files below; `Unscanned` = 0.
    pub files: u64,
    /// Unix seconds; for a `Dir`, the max of its own date and its children's.
    pub newest: i64,
    pub attached: bool,
    /// Context inherited from the parent folders (meaningful for a `Dir`).
    pub ctx: Context,
    /// Content type of a `File`; `None` for other kinds.
    pub ftype: Option<FileType>,
    /// Bytes per `FileType` below this node; allocated for `Dir` nodes only.
    types: Option<Box<[u64; 8]>>,
}

#[derive(Debug)]
pub struct EntryData {
    pub name: OsString,
    pub kind: NodeKind,
    pub size: u64,
    pub newest: i64,
}

pub type SharedTree = Arc<RwLock<Tree>>;

/// Arena tree: nodes live in a `Vec` and the index is the `NodeId`.
#[derive(Debug)]
pub struct Tree {
    nodes: Vec<Node>,
    root_path: PathBuf,
    generation: u64,
    root_is_drive: bool,
}

fn same_name(a: &OsStr, b: &OsStr) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

impl Tree {
    pub fn new(root_path: PathBuf, root_newest: i64) -> Tree {
        let root = Node {
            name: root_path.as_os_str().into(),
            parent: None,
            children: Vec::new(),
            kind: NodeKind::Dir,
            size: 0,
            files: 0,
            newest: root_newest,
            attached: true,
            ctx: Context::None,
            ftype: None,
            types: Some(Box::new([0; 8])),
        };
        let mut comps = root_path.components();
        let root_is_drive = matches!(
            (comps.next(), comps.next(), comps.next()),
            (
                Some(std::path::Component::Prefix(_)),
                Some(std::path::Component::RootDir),
                None
            )
        );
        Tree {
            nodes: vec![root],
            root_path,
            generation: 0,
            root_is_drive,
        }
    }

    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.0 as usize]
    }

    /// Incremented on every mutation.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Adds `entries` under `parent` (same order in the result) and propagates
    /// size, file count and newest date to all ancestors.
    pub fn add_children(&mut self, parent: NodeId, entries: Vec<EntryData>) -> Vec<NodeId> {
        let mut ids = Vec::with_capacity(entries.len());
        let (mut size, mut files, mut newest) = (0u64, 0u64, i64::MIN);
        let mut types = [0u64; 8];
        let parent_ctx = self.node(parent).ctx;
        let at_drive_root = self.root_is_drive && parent == self.root();
        for e in entries {
            let id = NodeId(u32::try_from(self.nodes.len()).expect("tree exceeds u32::MAX nodes"));
            let node_files = u64::from(e.kind == NodeKind::File);
            size += e.size;
            files += node_files;
            newest = newest.max(e.newest);
            let (ctx, ftype) = match e.kind {
                NodeKind::Dir => (child_context(parent_ctx, &e.name, at_drive_root), None),
                NodeKind::File => (parent_ctx, Some(file_type(parent_ctx, &e.name))),
                NodeKind::Unscanned => (Context::None, None),
            };
            if let Some(t) = ftype {
                types[t.index()] += e.size;
            }
            self.nodes.push(Node {
                name: e.name.into_boxed_os_str(),
                parent: Some(parent),
                children: Vec::new(),
                kind: e.kind,
                size: e.size,
                files: node_files,
                newest: e.newest,
                attached: true,
                ctx,
                ftype,
                types: (e.kind == NodeKind::Dir).then(|| Box::new([0; 8])),
            });
            self.node_mut(parent).children.push(id);
            ids.push(id);
        }
        if !ids.is_empty() {
            self.add_to_ancestors(Some(parent), size, files, newest, &types);
        }
        self.generation += 1;
        ids
    }

    fn add_to_ancestors(
        &mut self,
        from: Option<NodeId>,
        size: u64,
        files: u64,
        newest: i64,
        types: &[u64; 8],
    ) {
        let mut cur = from;
        while let Some(id) = cur {
            let n = self.node_mut(id);
            n.size += size;
            n.files += files;
            n.newest = n.newest.max(newest);
            if let Some(t) = n.types.as_mut() {
                for (a, b) in t.iter_mut().zip(types) {
                    *a += b;
                }
            }
            cur = n.parent;
        }
    }

    /// Removes `id` from its parent and subtracts its size and files from the
    /// ancestors. Ancestors' `newest` is deliberately not recomputed.
    pub fn detach(&mut self, id: NodeId) {
        let Some(parent) = self.node(id).parent else {
            return;
        };
        let (size, files) = (self.node(id).size, self.node(id).files);
        let types = self.type_bytes(id);
        let attached = self.is_attached(id);
        self.node_mut(parent).children.retain(|&c| c != id);
        if attached {
            let mut cur = Some(parent);
            while let Some(a) = cur {
                let n = self.node_mut(a);
                n.size -= size;
                n.files -= files;
                if let Some(t) = n.types.as_mut() {
                    for (a, b) in t.iter_mut().zip(&types) {
                        *a -= b;
                    }
                }
                cur = n.parent;
            }
        }
        let n = self.node_mut(id);
        n.parent = None;
        n.attached = false;
        self.generation += 1;
    }

    /// Moves `id` (with its subtree) under `new_parent`, updating aggregates.
    pub fn reparent(&mut self, id: NodeId, new_parent: NodeId) {
        self.detach(id);
        let (size, files, newest) = {
            let n = self.node(id);
            (n.size, n.files, n.newest)
        };
        let types = self.type_bytes(id);
        let n = self.node_mut(id);
        n.parent = Some(new_parent);
        n.attached = true;
        self.node_mut(new_parent).children.push(id);
        if self.is_attached(new_parent) {
            self.add_to_ancestors(Some(new_parent), size, files, newest, &types);
        }
        self.generation += 1;
    }

    /// Bytes per `FileType` that `id` contributes to its ancestors.
    fn type_bytes(&self, id: NodeId) -> [u64; 8] {
        let n = self.node(id);
        if let Some(t) = &n.types {
            **t
        } else {
            let mut a = [0; 8];
            if let Some(t) = n.ftype {
                a[t.index()] = n.size;
            }
            a
        }
    }

    /// Sets the context of the root. Call it before any child is added: it
    /// only affects nodes added afterwards.
    pub fn set_root_context(&mut self, ctx: Context) {
        self.node_mut(NodeId(0)).ctx = ctx;
    }

    /// Content type of a `File`; `None` for any other kind.
    pub fn file_type(&self, id: NodeId) -> Option<FileType> {
        self.node(id).ftype
    }

    /// For a `Dir`, the heaviest type and its share (0..=1) of the typed bytes.
    /// `None` when nothing is typed or `id` is not a `Dir`. Ties go to the
    /// lower `FileType` index.
    pub fn dominant_type(&self, id: NodeId) -> Option<(FileType, f32)> {
        let t = self.node(id).types.as_ref()?;
        let sum: u64 = t.iter().sum();
        if sum == 0 {
            return None;
        }
        let mut best = 0;
        for i in 1..8 {
            if t[i] > t[best] {
                best = i;
            }
        }
        Some((FileType::ALL[best], (t[best] as f64 / sum as f64) as f32))
    }

    pub fn path(&self, id: NodeId) -> PathBuf {
        let mut names = Vec::new();
        let mut cur = id;
        while let Some(p) = self.node(cur).parent {
            names.push(&*self.node(cur).name);
            cur = p;
        }
        let mut path = if cur == self.root() {
            self.root_path.clone()
        } else {
            PathBuf::new()
        };
        for name in names.into_iter().rev() {
            path.push(name);
        }
        path
    }

    /// True when walking up the parents reaches the root.
    pub fn is_attached(&self, id: NodeId) -> bool {
        let mut cur = id;
        while let Some(p) = self.node(cur).parent {
            cur = p;
        }
        cur == self.root()
    }

    /// Strict ancestry: a node is not its own ancestor.
    pub fn is_ancestor(&self, ancestor: NodeId, id: NodeId) -> bool {
        let mut cur = self.node(id).parent;
        while let Some(p) = cur {
            if p == ancestor {
                return true;
            }
            cur = self.node(p).parent;
        }
        false
    }

    /// Case-insensitive lookup of a direct child.
    pub fn child_by_name(&self, parent: NodeId, name: &OsStr) -> Option<NodeId> {
        self.node(parent)
            .children
            .iter()
            .copied()
            .find(|&c| same_name(&self.node(c).name, name))
    }

    /// The child of the root that is `id` or an ancestor of it; `None` for the root.
    pub fn top_level_of(&self, id: NodeId) -> Option<NodeId> {
        let mut cur = id;
        loop {
            let p = self.node(cur).parent?;
            if p == self.root() {
                return Some(cur);
            }
            cur = p;
        }
    }

    /// Creates or replaces the unique `Unscanned` child of the root.
    pub fn set_unscanned(&mut self, bytes: u64) {
        let root = self.root();
        let existing = self
            .node(root)
            .children
            .iter()
            .copied()
            .find(|&c| self.node(c).kind == NodeKind::Unscanned);
        match existing {
            Some(id) => {
                let old = self.node(id).size;
                self.node_mut(id).size = bytes;
                let r = self.node_mut(root);
                r.size = r.size - old + bytes;
                self.generation += 1;
            }
            None => {
                let newest = self.node(root).newest;
                self.add_children(
                    root,
                    vec![EntryData {
                        name: UNSCANNED_NAME.into(),
                        kind: NodeKind::Unscanned,
                        size: bytes,
                        newest,
                    }],
                );
            }
        }
    }

    /// Root size excluding the `Unscanned` node.
    pub fn scanned_size(&self) -> u64 {
        let root = self.node(self.root());
        let unscanned: u64 = root
            .children
            .iter()
            .map(|&c| self.node(c))
            .filter(|n| n.kind == NodeKind::Unscanned)
            .map(|n| n.size)
            .sum();
        root.size - unscanned
    }

    /// The root child named `$Recycle.Bin` (case-insensitive), if any.
    pub fn recycle_bin(&self) -> Option<NodeId> {
        self.child_by_name(self.root(), OsStr::new(RECYCLE_BIN_NAME))
    }

    pub fn ensure_recycle_bin(&mut self) -> NodeId {
        if let Some(id) = self.recycle_bin() {
            return id;
        }
        let root = self.root();
        let newest = self.node(root).newest;
        self.add_children(
            root,
            vec![EntryData {
                name: RECYCLE_BIN_NAME.into(),
                kind: NodeKind::Dir,
                size: 0,
                newest,
            }],
        )[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(n: &str, size: u64, t: i64) -> EntryData {
        EntryData {
            name: n.into(),
            kind: NodeKind::File,
            size,
            newest: t,
        }
    }
    fn d(n: &str, t: i64) -> EntryData {
        EntryData {
            name: n.into(),
            kind: NodeKind::Dir,
            size: 0,
            newest: t,
        }
    }
    // C:\ { Users { a.iso 1000 t300, b.txt 10 t200 }, pagefile.sys 16 t50 }
    fn sample() -> (Tree, NodeId, NodeId) {
        let mut t = Tree::new(PathBuf::from(r"C:\"), 100);
        let root = t.root();
        let users = t.add_children(root, vec![d("Users", 100), f("pagefile.sys", 16, 50)])[0];
        let a = t.add_children(users, vec![f("a.iso", 1000, 300), f("b.txt", 10, 200)])[0];
        (t, users, a)
    }

    #[test]
    fn add_children_propagates_aggregates() {
        let (t, users, a) = sample();
        assert_eq!(
            (
                t.node(t.root()).size,
                t.node(t.root()).files,
                t.node(t.root()).newest
            ),
            (1026, 3, 300)
        );
        assert_eq!((t.node(users).size, t.node(users).files), (1010, 2));
        assert_eq!(t.path(a), PathBuf::from(r"C:\Users\a.iso"));
    }

    #[test]
    fn detach_subtracts_from_ancestors() {
        let (mut t, users, a) = sample();
        let g = t.generation();
        t.detach(a);
        assert_eq!(t.node(t.root()).size, 26);
        assert_eq!(t.node(users).files, 1);
        assert!(!t.is_attached(a));
        assert!(t.generation() > g);
    }

    #[test]
    fn reparent_moves_size_under_recycle_bin() {
        let (mut t, users, a) = sample();
        let bin = t.ensure_recycle_bin();
        t.reparent(users, bin);
        assert_eq!(t.node(t.root()).size, 1026);
        assert_eq!(t.node(bin).size, 1010);
        assert_eq!(t.path(a), PathBuf::from(r"C:\$Recycle.Bin\Users\a.iso"));
    }

    #[test]
    fn ancestry_and_top_level() {
        let (t, users, a) = sample();
        assert!(t.is_ancestor(users, a));
        assert!(!t.is_ancestor(a, a));
        assert_eq!(t.top_level_of(a), Some(users));
        assert_eq!(t.top_level_of(t.root()), None);
        assert_eq!(t.child_by_name(t.root(), OsStr::new("users")), Some(users));
    }

    #[test]
    fn unscanned_counts_in_root_but_not_in_scanned_size() {
        let (mut t, _, _) = sample();
        t.set_unscanned(500);
        assert_eq!((t.node(t.root()).size, t.scanned_size()), (1526, 1026));
        t.set_unscanned(100);
        assert_eq!(t.node(t.root()).size, 1126);
    }

    #[test]
    fn recycle_bin_lookup_ignores_case() {
        let mut t = Tree::new(PathBuf::from(r"C:\"), 0);
        let root = t.root();
        let b = t.add_children(root, vec![d("$RECYCLE.BIN", 0)])[0];
        assert_eq!(t.recycle_bin(), Some(b));
        assert_eq!(t.ensure_recycle_bin(), b);
    }

    #[test]
    fn type_aggregation() {
        use crate::filetype::FileType;
        let mut t = Tree::new(PathBuf::from(r"C:\"), 0);
        let root = t.root();
        let top = t.add_children(root, vec![d("Users", 0), d("Windows", 0)]);
        let (users, win) = (top[0], top[1]);
        let u = t.add_children(users, vec![f("a.mkv", 100, 0), f("b.zip", 50, 0)]);
        let w = t.add_children(win, vec![f("c.dll", 30, 0)]);
        let close = |x: (FileType, f32), ty, share: f32| x.0 == ty && (x.1 - share).abs() < 1e-6;
        assert!(close(
            t.dominant_type(users).unwrap(),
            FileType::Media,
            100.0 / 150.0
        ));
        assert!(close(
            t.dominant_type(root).unwrap(),
            FileType::Media,
            100.0 / 180.0
        ));
        assert_eq!(t.file_type(w[0]), Some(FileType::System));
        assert_eq!(t.file_type(users), None);
        assert_eq!(t.dominant_type(u[0]), None);
        t.detach(u[0]);
        assert!(close(
            t.dominant_type(users).unwrap(),
            FileType::Archive,
            1.0
        ));
        let bin = t.ensure_recycle_bin();
        assert_eq!(t.dominant_type(bin), None);
        t.reparent(u[1], bin);
        assert!(close(t.dominant_type(bin).unwrap(), FileType::Archive, 1.0));
        assert_eq!(t.dominant_type(users), None);
    }

    #[test]
    fn root_context_is_inherited() {
        use crate::filetype::{Context, FileType};
        let mut t = Tree::new(PathBuf::from(r"D:\Lib"), 0);
        t.set_root_context(Context::Game);
        let root = t.root();
        let k = t.add_children(root, vec![f("a.mp4", 10, 0)]);
        assert_eq!(t.file_type(k[0]), Some(FileType::Game));
    }
}

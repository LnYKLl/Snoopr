//! A scan session: the shared tree, the running scan and the view state.

use snoopr_core::age::AgeFilter;
use snoopr_core::basket::Basket;
use snoopr_core::delete::recycle::query_recycle_bin;
use snoopr_core::scan::{ScanHandle, start_scan};
use snoopr_core::tree::{NodeId, NodeKind, SharedTree, Tree};
use snoopr_core::volume::{update_unscanned, volume_root_of, volume_usage};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub struct View {
    pub root: NodeId,
    pub selected: NodeId,
    pub age: AgeFilter,
}

pub struct Session {
    pub target: PathBuf,
    pub volume_root: PathBuf,
    pub tree: SharedTree,
    pub scan: Option<ScanHandle>,
    pub scan_time: Option<Duration>,
    pub scan_files: u64,
    pub denied: u64,
    pub view: View,
    pub basket: Basket,
    pub bin_bytes: u64,
    pub freed: u64,
    pub usage: (u64, u64),
    /// Disk size read once at start, for percentages while `usage` is unknown.
    pub disk_total: u64,
}

fn mtime_secs(path: &std::path::Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

impl Session {
    fn new(target: PathBuf, tree: Tree, scan: Option<ScanHandle>) -> Session {
        let root = tree.root();
        Session {
            volume_root: volume_root_of(&target),
            target,
            tree: Arc::new(RwLock::new(tree)),
            scan,
            scan_time: None,
            scan_files: 0,
            denied: 0,
            view: View {
                root,
                selected: root,
                age: AgeFilter::default(),
            },
            basket: Basket::default(),
            bin_bytes: 0,
            freed: 0,
            usage: (0, 0),
            disk_total: 0,
        }
    }

    /// Starts scanning `target` on worker threads; returns immediately.
    pub fn start(target: PathBuf) -> Session {
        let tree = Tree::new(target.clone(), mtime_secs(&target));
        let mut s = Session::new(target.clone(), tree, None);
        s.disk_total = volume_usage(&target).map_or(0, |u| u.0);
        let root = s.view.root;
        s.scan = Some(start_scan(s.tree.clone(), root, target));
        s
    }

    /// Cheap when nothing changed; finalizes the session once the scan is done.
    pub fn poll(&mut self) {
        let Some(scan) = &self.scan else { return };
        if !scan.is_done() {
            return;
        }
        let scan = self.scan.take().expect("checked above");
        self.scan_time = Some(scan.elapsed());
        self.scan_files = scan.progress.files.load(Ordering::Relaxed);
        self.denied = scan.progress.denied.load(Ordering::Relaxed);
        self.usage = volume_usage(&self.target).unwrap_or((0, 0));
        if let Ok(mut tree) = self.tree.write() {
            update_unscanned(&mut tree, self.usage.0, self.usage.1);
        }
        self.bin_bytes = query_recycle_bin(&self.volume_root).unwrap_or(0);
    }

    pub fn is_scanning(&self) -> bool {
        self.scan.is_some()
    }

    /// Total disk size for percentages: the final usage, else the one read at start; 0 if unknown.
    pub fn disk_size(&self) -> u64 {
        if self.usage.0 > 0 {
            self.usage.0
        } else {
            self.disk_total
        }
    }

    /// Stops a running scan (a dropped handle would keep scanning).
    pub fn cancel(&self) {
        if let Some(s) = &self.scan {
            s.cancel();
        }
    }

    pub fn zoom(&mut self, id: NodeId) {
        let is_dir = self
            .tree
            .read()
            .is_ok_and(|t| t.node(id).kind == NodeKind::Dir);
        if is_dir {
            self.view.root = id;
            self.view.selected = id;
        }
    }

    pub fn up(&mut self) {
        let parent = self
            .tree
            .read()
            .ok()
            .and_then(|t| t.node(self.view.root).parent);
        if let Some(p) = parent {
            self.view.root = p;
        }
    }

    pub fn select(&mut self, id: NodeId) {
        self.view.selected = id;
    }

    #[cfg(test)]
    pub fn from_tree(tree: Tree) -> Session {
        let target = tree.root_path().to_path_buf();
        Session::new(target, tree, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snoopr_core::tree::{EntryData, NodeKind, Tree};
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn e(n: &str, kind: NodeKind) -> EntryData {
        EntryData {
            name: OsString::from(n),
            kind,
            size: 10,
            newest: 1,
        }
    }

    #[test]
    fn session_navigation() {
        let mut t = Tree::new(PathBuf::from("C:\r"), 0);
        let root = t.root();
        let ids = t.add_children(root, vec![e("d", NodeKind::Dir), e("f", NodeKind::File)]);
        let (d, f) = (ids[0], ids[1]);
        let mut s = Session::from_tree(t);
        assert_eq!(s.view.root, root);
        s.zoom(f);
        assert_eq!((s.view.root, s.view.selected), (root, root));
        s.zoom(d);
        assert_eq!((s.view.root, s.view.selected), (d, d));
        s.up();
        assert_eq!(s.view.root, root);
        s.up();
        assert_eq!(s.view.root, root);
    }
}

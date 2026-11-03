//! A scan session: the shared tree, the running scan, the view state and the
//! deletion jobs (spec §7). Deletions, rescans and emptying the Recycle Bin run
//! on worker threads; `poll` collects their results without blocking.

use snoopr_core::actions::{PrimaryAction, primary_action};
use snoopr_core::age::AgeFilter;
use snoopr_core::basket::Basket;
use snoopr_core::cleanup::{
    Applied, DeleteItem, DeleteProgress, DeleteRequest, Deleter, Rescan, SystemDeleter,
    apply_outcomes, reset_view, run_delete,
};
use snoopr_core::delete::recycle::{empty_recycle_bin, query_recycle_bin};
use snoopr_core::delete::{DeleteMode, Failure, FailureReason, ItemOutcome};
use snoopr_core::filetype::root_context;
use snoopr_core::rules::RuleContext;
use snoopr_core::scan::{ScanHandle, start_scan};
use snoopr_core::tree::{EntryData, NodeId, NodeKind, SharedTree, Tree};
use snoopr_core::volume::{canonical_target, update_unscanned, volume_root_of, volume_usage};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

pub struct View {
    pub root: NodeId,
    pub selected: NodeId,
    pub age: AgeFilter,
}

/// A deletion can start only when nothing else runs and its subject has
/// `items` deletable items.
pub fn can_start_delete(busy: bool, items: usize) -> bool {
    !busy && items > 0
}

/// What a deletion is about: the whole basket, or one item that never enters it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Basket,
    One(NodeId),
}

/// The basket a deletion of `subject` is checked against: the real one, or one holding only the item.
pub fn subject_basket<'a>(tree: &Tree, basket: &'a Basket, subject: Subject) -> Cow<'a, Basket> {
    match subject {
        Subject::Basket => Cow::Borrowed(basket),
        Subject::One(id) => {
            let mut one = Basket::default();
            one.add(tree, id);
            Cow::Owned(one)
        }
    }
}

/// What the session is busy with, for the menus and the activity line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Activity {
    Idle,
    /// A scan or a rescan, running or waiting.
    Scanning,
    Deleting {
        done: usize,
        total: usize,
    },
    EmptyingBin,
}

/// Outcomes of each request of a job, in request order.
type DeleteResults = Vec<(DeleteMode, Vec<(NodeId, ItemOutcome)>)>;

/// A deletion running on a worker thread.
pub struct DeleteJob {
    /// Items done / items in all the requests of the job.
    pub progress: Arc<DeleteProgress>,
    rx: mpsc::Receiver<DeleteResults>,
}

/// A partially deleted folder being rescanned.
pub struct PendingRescan {
    pub handle: ScanHandle,
    pub node: NodeId,
    pub old_size: u64,
}

/// Partially deleted folders rescanned at the same time (each scan has its own thread pool).
const MAX_RESCANS: usize = 2;

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
    /// Safety rules, checked again for every item right before it is deleted.
    rules: RuleContext,
    pub delete: Option<DeleteJob>,
    pub rescans: Vec<PendingRescan>,
    /// Rescans waiting for a free slot (see `MAX_RESCANS`).
    queued_rescans: VecDeque<Rescan>,
    pub empty_job: Option<mpsc::Receiver<io::Result<()>>>,
    /// Failures not yet shown; see `take_failures`.
    pub failures: Vec<Failure>,
}

/// The items of `ids` that may really be deleted, with their path and size
/// read from the tree. An id is kept only when it is attached, in the basket,
/// and its primary action is `Remove`: never the root, the Recycle Bin or
/// anything in it, the "not scanned" block, a blocked item, or an item covered
/// by another basket item. Duplicates are dropped.
pub fn build_items(
    tree: &Tree,
    basket: &Basket,
    rules: &RuleContext,
    ids: &[NodeId],
) -> Vec<DeleteItem> {
    let mut out: Vec<DeleteItem> = Vec::new();
    for &id in ids {
        let deletable = tree.is_attached(id)
            && basket.contains(id)
            && primary_action(tree, basket, rules, id) == PrimaryAction::Remove;
        if deletable && !out.iter().any(|it| it.id == id) {
            out.push(DeleteItem {
                id,
                path: tree.path(id),
                size: tree.node(id).size,
            });
        }
    }
    out
}

/// Text of a caught panic, for the failure report.
fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".to_owned())
}

/// Runs the requests in order on the calling thread. `progress.total` counts
/// the items of all requests and `done` grows by one per item. A panic while
/// deleting an item becomes a failure of that item, so the job always reports.
fn run_requests(
    requests: &[DeleteRequest],
    deleter: &dyn Deleter,
    progress: &DeleteProgress,
) -> DeleteResults {
    let total = requests.iter().map(|r| r.items.len()).sum();
    progress.total.store(total, Ordering::SeqCst);
    let mut out = Vec::with_capacity(requests.len());
    for req in requests {
        let mut outcomes = Vec::with_capacity(req.items.len());
        for item in &req.items {
            // One item at a time, so the job's progress spans all requests.
            let one = DeleteRequest {
                items: vec![item.clone()],
                mode: req.mode,
            };
            let scratch = DeleteProgress::default();
            let outcome =
                match catch_unwind(AssertUnwindSafe(|| run_delete(&one, deleter, &scratch))) {
                    Ok(mut v) if v.len() == 1 => v.remove(0),
                    Ok(_) => (
                        item.id,
                        ItemOutcome::Failed(Failure {
                            path: item.path.clone(),
                            reason: FailureReason::Other("no outcome".to_owned()),
                        }),
                    ),
                    Err(e) => (
                        item.id,
                        ItemOutcome::Failed(Failure {
                            path: item.path.clone(),
                            reason: FailureReason::Other(panic_text(&*e)),
                        }),
                    ),
                };
            outcomes.push(outcome);
            progress.done.fetch_add(1, Ordering::SeqCst);
        }
        out.push((req.mode, outcomes));
    }
    out
}

fn mtime_secs(path: &std::path::Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// The tree a scan of `target` fills. Its root context (system, programs,
/// games…) comes from the safety rules and must be set before the first child.
fn empty_tree(target: &Path, newest: i64, rules: &RuleContext) -> Tree {
    let mut tree = Tree::new(target.to_path_buf(), newest);
    tree.set_root_context(root_context(target, rules));
    tree
}

impl Session {
    fn new(target: PathBuf, tree: Tree, scan: Option<ScanHandle>, rules: RuleContext) -> Session {
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
            rules,
            delete: None,
            rescans: Vec::new(),
            queued_rescans: VecDeque::new(),
            empty_job: None,
            failures: Vec::new(),
        }
    }

    /// Starts scanning `target` on worker threads; returns immediately.
    ///
    /// Every target (command line, folder picker, drive card, rescan) is first
    /// resolved to its real path ([`canonical_target`]): the safety rules read
    /// paths as text, so an alias such as a SUBST drive, a junction or an 8.3
    /// name must never reach them. A target that cannot be resolved (for
    /// instance a missing path) is not scanned: the session stays empty.
    pub fn start(target: PathBuf, rules: &RuleContext) -> Session {
        let Some(target) = canonical_target(&target) else {
            let tree = empty_tree(&target, 0, rules);
            let mut s = Session::new(target, tree, None, rules.clone());
            s.scan_time = Some(Duration::ZERO);
            return s;
        };
        let tree = empty_tree(&target, mtime_secs(&target), rules);
        let mut s = Session::new(target.clone(), tree, None, rules.clone());
        s.disk_total = volume_usage(&target).map_or(0, |u| u.0);
        let root = s.view.root;
        s.scan = Some(start_scan(s.tree.clone(), root, target));
        s
    }

    /// Cheap when nothing changed; collects whatever the workers finished.
    pub fn poll(&mut self) {
        self.poll_scan();
        self.poll_delete();
        self.poll_rescans();
        self.poll_empty_bin();
    }

    /// Finalizes the session once the scan is done.
    fn poll_scan(&mut self) {
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

    /// A scan, a rescan, a deletion or the emptying of the Recycle Bin is running.
    pub fn is_busy(&self) -> bool {
        self.is_scanning()
            || !self.rescans.is_empty()
            || !self.queued_rescans.is_empty()
            || self.is_deleting()
    }

    /// A deletion or the emptying of the Recycle Bin is running.
    pub fn is_deleting(&self) -> bool {
        self.delete.is_some() || self.empty_job.is_some()
    }

    /// What the session is busy with right now.
    pub fn activity(&self) -> Activity {
        if let Some(job) = &self.delete {
            Activity::Deleting {
                done: job.progress.done.load(Ordering::Relaxed),
                total: job.progress.total.load(Ordering::Relaxed),
            }
        } else if self.empty_job.is_some() {
            Activity::EmptyingBin
        } else if self.is_busy() {
            Activity::Scanning
        } else {
            Activity::Idle
        }
    }

    /// The ids a deletion of `subject` is about: the basket items, or the item itself.
    pub fn subject_ids(&self, subject: Subject) -> Vec<NodeId> {
        match subject {
            Subject::Basket => self.basket.items().to_vec(),
            Subject::One(id) => vec![id],
        }
    }

    /// The deletable items among `ids` for `subject` (see [`build_items`]): path and size from the tree.
    pub fn delete_items(&self, subject: Subject, ids: &[NodeId]) -> Vec<DeleteItem> {
        let tree = self.tree.read().unwrap_or_else(PoisonError::into_inner);
        let basket = subject_basket(&tree, &self.basket, subject);
        build_items(&tree, &basket, &self.rules, ids)
    }

    /// Checks every item of `requests` again against the tree, the safety rules
    /// and the basket of `subject`; its path is read from the tree, never taken
    /// from the caller. Anything else is dropped, and an id only runs once.
    fn checked_requests(
        &self,
        subject: Subject,
        requests: Vec<DeleteRequest>,
    ) -> Vec<DeleteRequest> {
        let tree = self.tree.read().unwrap_or_else(PoisonError::into_inner);
        let basket = subject_basket(&tree, &self.basket, subject);
        let mut seen: Vec<NodeId> = Vec::new();
        let mut out = Vec::new();
        for r in requests {
            let ids: Vec<NodeId> = r
                .items
                .iter()
                .map(|it| it.id)
                .filter(|id| !seen.contains(id))
                .collect();
            let items = build_items(&tree, &basket, &self.rules, &ids);
            seen.extend(items.iter().map(|it| it.id));
            if !items.is_empty() {
                out.push(DeleteRequest {
                    items,
                    mode: r.mode,
                });
            }
        }
        out
    }

    /// Starts deleting on a worker thread; the requests run in order. Every
    /// item is checked again here (see [`Session::checked_requests`]). Does
    /// nothing while busy or when no item is left.
    pub fn start_delete(&mut self, subject: Subject, requests: Vec<DeleteRequest>) {
        if self.is_busy() {
            return;
        }
        let requests = self.checked_requests(subject, requests);
        if requests.is_empty() {
            return;
        }
        let progress = Arc::new(DeleteProgress::default());
        let total = requests.iter().map(|r| r.items.len()).sum();
        progress.total.store(total, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        let worker_progress = progress.clone();
        std::thread::spawn(move || {
            let out = run_requests(&requests, &SystemDeleter, &worker_progress);
            let _ = tx.send(out);
        });
        self.delete = Some(DeleteJob { progress, rx });
    }

    /// Starts emptying the Recycle Bin of this volume on a worker thread.
    /// Does nothing while busy.
    pub fn start_empty_bin(&mut self) {
        if self.is_busy() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let root = self.volume_root.clone();
        std::thread::spawn(move || {
            let _ = tx.send(empty_recycle_bin(&root));
        });
        self.empty_job = Some(rx);
    }

    /// Failures collected since the last call, for the recap dialog.
    pub fn take_failures(&mut self) -> Vec<Failure> {
        std::mem::take(&mut self.failures)
    }

    fn poll_delete(&mut self) {
        let Some(job) = &self.delete else { return };
        let results = match job.rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            // The worker always sends; if it vanished there is nothing to apply.
            Err(TryRecvError::Disconnected) => Vec::new(),
        };
        self.delete = None;
        self.apply_delete_results(results);
    }

    /// Mirrors finished deletions in the tree without rescanning (spec §7.1),
    /// then rescans the partially deleted folders and reads the disk again.
    fn apply_delete_results(&mut self, results: DeleteResults) {
        let mut applied = Applied::default();
        {
            // Held for `apply_outcomes` only: no I/O under the write lock.
            let mut tree = self.tree.write().unwrap_or_else(PoisonError::into_inner);
            for (mode, outcomes) in &results {
                let a = apply_outcomes(&mut tree, *mode, outcomes);
                applied.freed += a.freed;
                applied.failures.extend(a.failures);
                applied.rescans.extend(a.rescans);
                applied.removed.extend(a.removed);
            }
        }
        self.freed += applied.freed;
        {
            let tree = self.tree.read().unwrap_or_else(PoisonError::into_inner);
            let removed = &applied.removed;
            // Failed items stay. Also drop anything that left the tree or went
            // to the Recycle Bin with a removed folder.
            let bin = tree.recycle_bin();
            self.basket.retain(|id| {
                !removed.contains(&id)
                    && tree.is_attached(id)
                    && bin.is_none_or(|b| b != id && !tree.is_ancestor(b, id))
            });
            (self.view.root, self.view.selected) =
                reset_view(&tree, removed, self.view.root, self.view.selected);
        }
        self.queued_rescans.extend(applied.rescans);
        self.start_queued_rescans();
        self.refresh_volume();
        self.bin_bytes = query_recycle_bin(&self.volume_root).unwrap_or(self.bin_bytes);
        self.failures.extend(applied.failures);
    }

    fn start_queued_rescans(&mut self) {
        while self.rescans.len() < MAX_RESCANS
            && let Some(r) = self.queued_rescans.pop_front()
        {
            let handle = start_scan(self.tree.clone(), r.node, r.path);
            self.rescans.push(PendingRescan {
                handle,
                node: r.node,
                old_size: r.old_size,
            });
        }
    }

    /// What a finished rescan no longer finds was freed.
    fn poll_rescans(&mut self) {
        let mut finished = Vec::new();
        let mut i = 0;
        while i < self.rescans.len() {
            if self.rescans[i].handle.is_done() {
                finished.push(self.rescans.swap_remove(i));
            } else {
                i += 1;
            }
        }
        if finished.is_empty() {
            return;
        }
        {
            let tree = self.tree.read().unwrap_or_else(PoisonError::into_inner);
            for r in &finished {
                self.freed += r.old_size.saturating_sub(tree.node(r.node).size);
            }
        }
        self.start_queued_rescans();
        self.refresh_volume();
    }

    /// Reads the disk usage again (outside any lock) and, once no rescan is
    /// pending, updates the "not scanned" block of a drive scan.
    fn refresh_volume(&mut self) {
        if let Ok(u) = volume_usage(&self.volume_root) {
            self.usage = u;
        }
        let rescanning = !self.rescans.is_empty() || !self.queued_rescans.is_empty();
        if self.usage.0 > 0 && !rescanning {
            let mut tree = self.tree.write().unwrap_or_else(PoisonError::into_inner);
            update_unscanned(&mut tree, self.usage.0, self.usage.1);
        }
    }

    fn poll_empty_bin(&mut self) {
        let Some(rx) = &self.empty_job else { return };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(io::Error::from(io::ErrorKind::Interrupted)),
        };
        self.empty_job = None;
        self.finish_empty_bin(result);
    }

    /// Success: the bin's bytes are freed and its block is rescanned.
    /// Failure: reported in the recap dialog.
    fn finish_empty_bin(&mut self, result: io::Result<()>) {
        match result {
            Ok(()) => {
                self.freed += self.bin_bytes;
                self.bin_bytes = 0;
                // Only this user's bin was emptied: the other folders of
                // `$Recycle.Bin` (other accounts, orphans) stay on disk. The
                // block is replaced by an empty folder that is rescanned, so
                // they stay in the tree; the rescan adds nothing to `freed`.
                let replaced = {
                    let mut tree = self.tree.write().unwrap_or_else(PoisonError::into_inner);
                    tree.recycle_bin().map(|b| {
                        let (name, parent, newest, path) = {
                            let n = tree.node(b);
                            (n.name.to_os_string(), n.parent, n.newest, tree.path(b))
                        };
                        tree.detach(b);
                        let rescan = parent.map(|p| {
                            let entry = EntryData {
                                name,
                                kind: NodeKind::Dir,
                                size: 0,
                                newest,
                            };
                            let node = tree.add_children(p, vec![entry])[0];
                            Rescan {
                                node,
                                path,
                                old_size: 0,
                            }
                        });
                        (b, rescan)
                    })
                };
                if let Some((b, rescan)) = replaced {
                    {
                        let tree = self.tree.read().unwrap_or_else(PoisonError::into_inner);
                        self.basket.retain(|id| tree.is_attached(id));
                        (self.view.root, self.view.selected) =
                            reset_view(&tree, &[b], self.view.root, self.view.selected);
                    }
                    self.queued_rescans.extend(rescan);
                    self.start_queued_rescans();
                }
                self.refresh_volume();
            }
            Err(e) => {
                self.failures.push(Failure {
                    path: self.volume_root.clone(),
                    reason: FailureReason::Other(e.to_string()),
                });
                self.bin_bytes = query_recycle_bin(&self.volume_root).unwrap_or(self.bin_bytes);
            }
        }
    }

    /// Total disk size for percentages: the final usage, else the one read at start; 0 if unknown.
    pub fn disk_size(&self) -> u64 {
        if self.usage.0 > 0 {
            self.usage.0
        } else {
            self.disk_total
        }
    }

    /// Stops a running scan and the rescans (a dropped handle would keep scanning).
    /// A deletion already started runs to its end.
    pub fn cancel(&self) {
        if let Some(s) = &self.scan {
            s.cancel();
        }
        for r in &self.rescans {
            r.handle.cancel();
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
        let rules = RuleContext {
            windows_dir: r"C:\Windows".into(),
            program_data: r"C:\ProgramData".into(),
            users_dir: r"C:\Users".into(),
            cloud_roots: Vec::new(),
        };
        Session::new(target, tree, None, rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snoopr_core::filetype::{Context, FileType};
    use snoopr_core::rules::RuleContext;
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

    #[test]
    fn scan_tree_gets_root_context() {
        let rules = RuleContext {
            windows_dir: r"C:\Windows".into(),
            program_data: r"C:\ProgramData".into(),
            users_dir: r"C:\Users".into(),
            cloud_roots: Vec::new(),
        };
        let mut t = empty_tree(Path::new(r"C:\Windows"), 7, &rules);
        let root = t.root();
        assert_eq!(t.node(root).ctx, Context::System);
        assert_eq!(t.node(root).newest, 7);
        let f = t.add_children(root, vec![e("a.zip", NodeKind::File)])[0];
        assert_eq!(t.file_type(f), Some(FileType::System));
        let t = empty_tree(Path::new(r"D:\Data"), 0, &rules);
        assert_eq!(t.node(t.root()).ctx, Context::None);
    }

    /// The target is resolved before anything else (spec §8: the safety rules
    /// must see the real path, not an alias such as a SUBST drive).
    #[test]
    fn start_scans_the_resolved_target() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a").join("b")).unwrap();
        let real = canonical_target(&dir.path().join("a")).unwrap();
        let messy = dir.path().join(".").join("a").join("b").join("..");
        let mut s = Session::start(messy, &test_rules());
        assert_eq!(s.target, real);
        assert_eq!(s.tree.read().unwrap().root_path(), real);
        wait_idle(&mut s);
        let t = s.tree.read().unwrap();
        assert!(t.child_by_name(t.root(), "b".as_ref()).is_some());
    }

    /// A target that cannot be resolved is not scanned at all.
    #[test]
    fn start_on_an_unresolvable_target_scans_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");
        let s = Session::start(missing.clone(), &test_rules());
        assert!(s.scan.is_none() && !s.is_busy());
        assert_eq!(s.target, missing);
        assert_eq!(s.scan_time, Some(Duration::ZERO));
        let t = s.tree.read().unwrap();
        assert!(t.node(t.root()).children.is_empty());
    }

    #[test]
    fn delete_blocked_while_busy() {
        assert!(!can_start_delete(true, 3));
        assert!(!can_start_delete(false, 0));
        assert!(can_start_delete(false, 1));
    }

    fn sized(n: &str, kind: NodeKind, size: u64) -> EntryData {
        EntryData {
            name: OsString::from(n),
            kind,
            size,
            newest: 1,
        }
    }

    fn test_rules() -> RuleContext {
        RuleContext {
            windows_dir: r"C:\Windows".into(),
            program_data: r"C:\ProgramData".into(),
            users_dir: r"C:\Users".into(),
            cloud_roots: Vec::new(),
        }
    }

    /// Polls until nothing runs any more (60 s at most).
    fn wait_idle(s: &mut Session) {
        let start = std::time::Instant::now();
        loop {
            s.poll();
            if !s.is_busy() {
                return;
            }
            assert!(start.elapsed() < Duration::from_secs(60), "still busy");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn build_items_keeps_only_deletable_basket_items() {
        use snoopr_core::basket::Basket;
        let mut t = Tree::new(PathBuf::from(r"C:\"), 0);
        let root = t.root();
        let top = t.add_children(
            root,
            vec![
                sized("Windows", NodeKind::Dir, 0),
                sized("$Recycle.Bin", NodeKind::Dir, 0),
                sized("Data", NodeKind::Dir, 0),
            ],
        );
        let (windows, bin, data) = (top[0], top[1], top[2]);
        t.add_children(windows, vec![sized("w.dll", NodeKind::File, 5)]);
        let old = t.add_children(bin, vec![sized("old.zip", NodeKind::File, 7)])[0];
        let files = t.add_children(
            data,
            vec![
                sized("big.iso", NodeKind::File, 100),
                sized("other.bin", NodeKind::File, 3),
                sized("gone.bin", NodeKind::File, 1),
            ],
        );
        let (big, other, gone) = (files[0], files[1], files[2]);
        t.set_unscanned(50);
        let unscanned = t
            .node(root)
            .children
            .iter()
            .copied()
            .find(|&c| t.node(c).kind == NodeKind::Unscanned)
            .unwrap();
        // The basket API itself does not apply the safety rules.
        let mut b = Basket::default();
        for id in [windows, old, unscanned, big, gone] {
            b.add(&t, id);
        }
        t.detach(gone);
        let rules = test_rules();
        let ids = [root, windows, bin, old, unscanned, big, other, gone, big];
        let items = build_items(&t, &b, &rules, &ids);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, big);
        assert_eq!(items[0].path, PathBuf::from(r"C:\Data\big.iso"));
        assert_eq!(items[0].size, 100);
        // The root and the Recycle Bin are dropped even when they are the basket.
        let mut only_root = Basket::default();
        only_root.add(&t, root);
        assert!(build_items(&t, &only_root, &rules, &[root]).is_empty());
        let mut only_bin = Basket::default();
        only_bin.add(&t, bin);
        assert!(build_items(&t, &only_bin, &rules, &[bin]).is_empty());
    }

    struct OneFixture {
        s: Session,
        root: NodeId,
        movie: NodeId,
        b: NodeId,
        windows: NodeId,
        bin: NodeId,
        old: NodeId,
        unscanned: NodeId,
    }

    fn one_fixture() -> OneFixture {
        let mut t = Tree::new(PathBuf::from(r"C:\"), 0);
        let root = t.root();
        let top = t.add_children(
            root,
            vec![
                sized("Data", NodeKind::Dir, 0),
                sized("Windows", NodeKind::Dir, 0),
                sized("$Recycle.Bin", NodeKind::Dir, 0),
            ],
        );
        let (data, windows, bin) = (top[0], top[1], top[2]);
        let files = t.add_children(
            data,
            vec![
                sized("movie.mkv", NodeKind::File, 100),
                sized("b.zip", NodeKind::File, 50),
            ],
        );
        t.add_children(windows, vec![sized("w.dll", NodeKind::File, 10)]);
        let old = t.add_children(bin, vec![sized("old.zip", NodeKind::File, 5)])[0];
        t.set_unscanned(7);
        let unscanned = t
            .node(root)
            .children
            .iter()
            .copied()
            .find(|&c| t.node(c).kind == NodeKind::Unscanned)
            .unwrap();
        OneFixture {
            s: Session::from_tree(t),
            root,
            movie: files[0],
            b: files[1],
            windows,
            bin,
            old,
            unscanned,
        }
    }

    fn add_to_basket(s: &mut Session, ids: &[NodeId]) {
        let tree = s.tree.clone();
        let tree = tree.read().unwrap();
        for &id in ids {
            s.basket.add(&tree, id);
        }
    }

    fn ids(items: Vec<DeleteItem>) -> Vec<NodeId> {
        items.into_iter().map(|it| it.id).collect()
    }

    /// One request per id; the caller's path and size are placeholders.
    fn requests_for(ids: &[NodeId]) -> Vec<DeleteRequest> {
        ids.iter()
            .map(|&id| DeleteRequest {
                items: vec![DeleteItem {
                    id,
                    path: PathBuf::new(),
                    size: 0,
                }],
                mode: DeleteMode::Permanent,
            })
            .collect()
    }

    fn request_ids(reqs: &[DeleteRequest]) -> Vec<NodeId> {
        reqs.iter()
            .flat_map(|r| r.items.iter().map(|it| it.id))
            .collect()
    }

    #[test]
    fn delete_items_for_one_checks_like_a_one_item_basket() {
        let f = one_fixture();
        let s = &f.s;
        assert_eq!(
            ids(s.delete_items(Subject::One(f.movie), &[f.movie])),
            vec![f.movie]
        );
        for id in [f.windows, f.root, f.bin, f.old, f.unscanned] {
            assert!(s.delete_items(Subject::One(id), &[id]).is_empty(), "{id:?}");
        }
        // An id other than the subject is never kept.
        assert!(s.delete_items(Subject::One(f.movie), &[f.b]).is_empty());
        assert!(s.basket.is_empty());
    }

    #[test]
    fn checked_requests_one_keeps_only_the_subject() {
        let mut f = one_fixture();
        add_to_basket(&mut f.s, &[f.movie, f.b]);
        let s = &f.s;
        let both = [f.movie, f.b];
        let out = s.checked_requests(Subject::One(f.movie), requests_for(&both));
        assert_eq!(request_ids(&out), vec![f.movie]);
        assert_eq!(s.basket.items(), &[f.movie, f.b]);
        let out = s.checked_requests(Subject::Basket, requests_for(&both));
        assert_eq!(request_ids(&out), vec![f.movie, f.b]);
    }

    #[test]
    fn subject_ids_lists_what_the_dialog_shows() {
        let mut f = one_fixture();
        add_to_basket(&mut f.s, &[f.movie, f.b]);
        assert_eq!(f.s.subject_ids(Subject::One(f.b)), vec![f.b]);
        assert_eq!(
            f.s.subject_ids(Subject::Basket),
            f.s.basket.items().to_vec()
        );
    }

    #[derive(Default)]
    struct FakeDeleter {
        calls: std::sync::Mutex<Vec<(DeleteMode, PathBuf)>>,
    }

    impl Deleter for FakeDeleter {
        fn recycle(&self, path: &Path) -> Result<(), FailureReason> {
            self.calls
                .lock()
                .unwrap()
                .push((DeleteMode::RecycleBin, path.to_path_buf()));
            Ok(())
        }
        fn delete_permanently(&self, path: &Path) -> ItemOutcome {
            self.calls
                .lock()
                .unwrap()
                .push((DeleteMode::Permanent, path.to_path_buf()));
            if path.ends_with("boom") {
                panic!("boom");
            }
            ItemOutcome::Removed
        }
    }

    fn item(i: u32) -> DeleteItem {
        DeleteItem {
            id: NodeId(i),
            path: PathBuf::from(format!(r"C:\x{i}")),
            size: 1,
        }
    }

    #[test]
    fn run_requests_keeps_order_and_counts_every_item() {
        let mut boom = item(4);
        boom.path = PathBuf::from(r"C:\boom");
        let reqs = vec![
            DeleteRequest {
                items: vec![item(1), item(2)],
                mode: DeleteMode::RecycleBin,
            },
            DeleteRequest {
                items: vec![item(3), boom],
                mode: DeleteMode::Permanent,
            },
        ];
        let fake = FakeDeleter::default();
        let progress = DeleteProgress::default();
        let out = run_requests(&reqs, &fake, &progress);
        assert_eq!(progress.total.load(Ordering::SeqCst), 4);
        assert_eq!(progress.done.load(Ordering::SeqCst), 4);
        let calls = fake.calls.lock().unwrap().clone();
        let modes: Vec<_> = calls.iter().map(|c| c.0).collect();
        use DeleteMode::{Permanent, RecycleBin};
        assert_eq!(modes, [RecycleBin, RecycleBin, Permanent, Permanent]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, RecycleBin);
        assert_eq!(out[1].0, Permanent);
        let ids: Vec<_> = out[0].1.iter().map(|o| o.0).collect();
        assert_eq!(ids, [NodeId(1), NodeId(2)]);
        assert_eq!(out[1].1[0], (NodeId(3), ItemOutcome::Removed));
        // A panic while deleting becomes a failure of that item only.
        let (id, outcome) = &out[1].1[1];
        assert_eq!(*id, NodeId(4));
        assert!(matches!(
            outcome,
            ItemOutcome::Failed(Failure { reason: FailureReason::Other(m), .. }) if m.contains("boom")
        ));
    }

    /// A tree rooted at a folder that does not exist, so a rescan finds nothing.
    const NOWHERE: &str = r"C:\snoopr-unit-test-does-not-exist";

    #[test]
    fn delete_results_update_tree_basket_view_and_freed() {
        let mut t = Tree::new(PathBuf::from(NOWHERE), 0);
        let root = t.root();
        let top = t.add_children(
            root,
            vec![
                sized("a", NodeKind::Dir, 0),
                sized("b.bin", NodeKind::File, 50),
                sized("c", NodeKind::Dir, 0),
            ],
        );
        let (a, b, c) = (top[0], top[1], top[2]);
        let f = t.add_children(a, vec![sized("f", NodeKind::File, 100)])[0];
        t.add_children(c, vec![sized("g", NodeKind::File, 30)]);
        let mut s = Session::from_tree(t);
        {
            let tree = s.tree.clone();
            let tree = tree.read().unwrap();
            for id in [a, b, c] {
                s.basket.add(&tree, id);
            }
        }
        s.view.root = a;
        s.view.selected = f;
        let fail_b = Failure {
            path: PathBuf::from(NOWHERE).join("b.bin"),
            reason: FailureReason::InUse,
        };
        let fail_g = Failure {
            path: PathBuf::from(NOWHERE).join(r"c\g"),
            reason: FailureReason::AccessDenied,
        };
        s.apply_delete_results(vec![(
            DeleteMode::Permanent,
            vec![
                (a, ItemOutcome::Removed),
                (b, ItemOutcome::Failed(fail_b.clone())),
                (c, ItemOutcome::Partial(vec![fail_g.clone()])),
            ],
        )]);
        assert_eq!(s.freed, 100);
        // Failed items stay in the basket; removed and partial ones leave it.
        assert_eq!(s.basket.items(), &[b]);
        assert_eq!((s.view.root, s.view.selected), (root, root));
        assert!(s.is_busy(), "the partial folder is being rescanned");
        assert_eq!(s.take_failures(), vec![fail_b, fail_g]);
        assert!(s.take_failures().is_empty());
        wait_idle(&mut s);
        // Nothing is left on disk for `c`: its 30 bytes count as freed.
        assert_eq!(s.freed, 130);
        let tree = s.tree.read().unwrap();
        assert!(!tree.is_attached(a));
        let ph = tree.child_by_name(root, "c".as_ref()).unwrap();
        assert_ne!(ph, c);
        assert_eq!(tree.node(root).size, 50);
    }

    #[test]
    fn start_delete_refuses_unsafe_or_busy() {
        let mut t = Tree::new(PathBuf::from(NOWHERE), 0);
        let root = t.root();
        let d = t.add_children(root, vec![sized("d", NodeKind::Dir, 0)])[0];
        let mut s = Session::from_tree(t);
        // The root, even with a forged item, never reaches a worker.
        s.start_delete(
            Subject::Basket,
            vec![DeleteRequest {
                items: vec![DeleteItem {
                    id: root,
                    path: PathBuf::from(NOWHERE),
                    size: 0,
                }],
                mode: DeleteMode::Permanent,
            }],
        );
        assert!(s.delete.is_none());
        // Nor does an item that is not in the basket.
        s.start_delete(
            Subject::Basket,
            vec![DeleteRequest {
                items: vec![DeleteItem {
                    id: d,
                    path: PathBuf::from(NOWHERE).join("d"),
                    size: 0,
                }],
                mode: DeleteMode::Permanent,
            }],
        );
        assert!(s.delete.is_none());
        // Nothing starts while a rescan runs.
        {
            let tree = s.tree.clone();
            s.basket.add(&tree.read().unwrap(), d);
        }
        s.rescans.push(PendingRescan {
            handle: start_scan(s.tree.clone(), d, PathBuf::from(NOWHERE).join("d")),
            node: d,
            old_size: 0,
        });
        assert!(s.is_busy());
        let items = s.delete_items(Subject::Basket, &[d]);
        assert_eq!(items.len(), 1);
        s.start_delete(
            Subject::Basket,
            vec![DeleteRequest {
                items,
                mode: DeleteMode::Permanent,
            }],
        );
        assert!(s.delete.is_none());
        wait_idle(&mut s);
    }

    /// A drive root that does not exist here, so rescanning its
    /// `$Recycle.Bin` reads nothing (never the real Recycle Bin).
    fn missing_drive() -> PathBuf {
        ('D'..='Z')
            .rev()
            .map(|c| PathBuf::from(format!(r"{c}:\")))
            .find(|p| !p.exists())
            .expect("a free drive letter")
    }

    /// Only this user's bin is emptied (`SHEmptyRecycleBinW`): the block is
    /// rescanned instead of removed, so other accounts' folders stay. Calls
    /// `finish_empty_bin` directly, never the real API.
    #[test]
    fn emptied_bin_is_counted_and_rescanned() {
        let drive = missing_drive();
        let mut t = Tree::new(drive.clone(), 0);
        let root = t.root();
        let top = t.add_children(
            root,
            vec![
                sized("$Recycle.Bin", NodeKind::Dir, 0),
                sized("d", NodeKind::File, 10),
            ],
        );
        let bin = top[0];
        let x = t.add_children(bin, vec![sized("x", NodeKind::File, 40)])[0];
        let mut s = Session::from_tree(t);
        s.bin_bytes = 40;
        s.view.root = bin;
        s.view.selected = x;
        s.finish_empty_bin(Ok(()));
        assert_eq!((s.freed, s.bin_bytes), (40, 0));
        assert_eq!((s.view.root, s.view.selected), (root, root));
        let ph = {
            let tree = s.tree.read().unwrap();
            let ph = tree.recycle_bin().expect("a new $Recycle.Bin block");
            assert_ne!(ph, bin);
            assert!(!tree.is_attached(bin));
            let n = tree.node(ph);
            assert_eq!(&*n.name, std::ffi::OsStr::new("$Recycle.Bin"));
            assert_eq!((n.parent, n.kind, n.size), (Some(root), NodeKind::Dir, 0));
            assert_eq!(tree.node(root).size, 10);
            ph
        };
        let rescanned =
            s.rescans.iter().any(|r| r.node == ph) || s.queued_rescans.iter().any(|r| r.node == ph);
        assert!(rescanned && s.is_busy());
        wait_idle(&mut s);
        // The rescan of the emptied bin adds nothing to `freed`.
        assert_eq!(s.freed, 40);
        assert!(s.take_failures().is_empty());
        s.finish_empty_bin(Err(std::io::Error::other("nope")));
        assert_eq!(s.freed, 40);
        assert_eq!(
            s.take_failures(),
            vec![Failure {
                path: drive,
                reason: FailureReason::Other("nope".into()),
            }]
        );
    }

    /// Permanent deletion end to end on real files: `big` goes, `locked` keeps
    /// only its locked file and is rescanned. Never touches the Recycle Bin.
    #[cfg(windows)]
    fn delete_big_and_locked_permanently(dir: &Path) {
        let rules = RuleContext::from_system();
        let mut s = Session::start(dir.to_path_buf(), &rules);
        wait_idle(&mut s);
        let tree = s.tree.clone();
        let (root, big, locked, big_size, locked_size) = {
            let t = tree.read().unwrap();
            let r = t.root();
            let big = t.child_by_name(r, "big".as_ref()).expect("big");
            let locked = t.child_by_name(r, "locked".as_ref()).expect("locked");
            for id in [big, locked] {
                s.basket.add(&t, id);
            }
            (r, big, locked, t.node(big).size, t.node(locked).size)
        };
        assert!(big_size > 0);
        let items = s.delete_items(Subject::Basket, &[big, locked]);
        assert_eq!(items.len(), 2);
        s.start_delete(
            Subject::Basket,
            vec![DeleteRequest {
                items,
                mode: DeleteMode::Permanent,
            }],
        );
        assert!(s.delete.is_some() && s.is_busy());
        wait_idle(&mut s);
        assert!(!dir.join("big").exists());
        assert!(dir.join("locked").join("x.bin").exists());
        let failures = s.take_failures();
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert_eq!(failures[0].reason, FailureReason::InUse);
        assert!(failures[0].path.ends_with(r"locked\x.bin"));
        assert!(s.basket.is_empty());
        let t = tree.read().unwrap();
        assert!(!t.is_attached(big));
        assert!(t.child_by_name(root, "big".as_ref()).is_none());
        let ph = t.child_by_name(root, "locked".as_ref()).expect("rescanned");
        assert_ne!(ph, locked);
        let names: Vec<_> = t
            .node(ph)
            .children
            .iter()
            .map(|&c| t.node(c).name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["x.bin"]);
        let left = t.node(ph).size;
        assert!(left > 0 && left < locked_size);
        assert_eq!(s.freed, big_size + locked_size - left);
        assert_eq!(t.node(root).size, left);
    }

    #[cfg(windows)]
    #[test]
    fn permanent_delete_end_to_end() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join("big")).unwrap();
        std::fs::create_dir_all(p.join("locked")).unwrap();
        for i in 0..3 {
            std::fs::write(p.join("big").join(format!("f{i}.bin")), vec![0u8; 64 << 10]).unwrap();
        }
        std::fs::write(p.join("locked").join("x.bin"), vec![0u8; 8 << 10]).unwrap();
        std::fs::write(p.join("locked").join("free.bin"), vec![0u8; 64 << 10]).unwrap();
        let _held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(p.join("locked").join("x.bin"))
            .unwrap();
        delete_big_and_locked_permanently(p);
    }

    /// Manual check against a prepared throwaway folder whose name must be
    /// `snoopr-test` (`big\` and `locked\x.bin` held open by another process):
    /// `SNOOPR_E2E_DIR=C:\snoopr-test cargo test -p snoopr-app -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore = "manual: permanently deletes big and locked in SNOOPR_E2E_DIR"]
    fn permanent_delete_on_prepared_folder() {
        let dir = PathBuf::from(std::env::var_os("SNOOPR_E2E_DIR").expect("SNOOPR_E2E_DIR"));
        assert_eq!(dir.file_name(), Some("snoopr-test".as_ref()));
        delete_big_and_locked_permanently(&dir);
    }

    #[test]
    fn activity_follows_the_jobs() {
        let mut s = one_fixture().s;
        assert_eq!(s.activity(), Activity::Idle);
        let progress = Arc::new(DeleteProgress::default());
        progress.total.store(2, Ordering::SeqCst);
        let (_tx, rx) = mpsc::channel();
        s.delete = Some(DeleteJob { progress, rx });
        assert_eq!(s.activity(), Activity::Deleting { done: 0, total: 2 });
    }
}

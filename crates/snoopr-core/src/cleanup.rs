//! Orchestration de la suppression et mise à jour de l'arbre.

use crate::delete::{DeleteMode, Failure, FailureReason, ItemOutcome, permanent, recycle};
use crate::tree::{EntryData, NodeId, NodeKind, Tree};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Abstraction des suppressions, pour tester sans toucher au disque.
pub trait Deleter: Send + Sync {
    fn recycle(&self, path: &Path) -> Result<(), FailureReason>;
    fn delete_permanently(&self, path: &Path) -> ItemOutcome;
}

/// Suppression réelle : délègue à `recycle::recycle` et `permanent::delete_permanently`.
pub struct SystemDeleter;

impl Deleter for SystemDeleter {
    fn recycle(&self, path: &Path) -> Result<(), FailureReason> {
        recycle::recycle(path)
    }
    fn delete_permanently(&self, path: &Path) -> ItemOutcome {
        permanent::delete_permanently(path)
    }
}

#[derive(Clone, Debug)]
pub struct DeleteItem {
    pub id: NodeId,
    pub path: PathBuf,
    pub size: u64,
}

#[derive(Clone, Debug)]
pub struct DeleteRequest {
    pub items: Vec<DeleteItem>,
    pub mode: DeleteMode,
}

#[derive(Default)]
pub struct DeleteProgress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
}

/// Exécute la suppression élément par élément et fait avancer `progress`.
pub fn run_delete(
    req: &DeleteRequest,
    deleter: &dyn Deleter,
    progress: &DeleteProgress,
) -> Vec<(NodeId, ItemOutcome)> {
    progress.total.store(req.items.len(), Ordering::SeqCst);
    let mut out = Vec::with_capacity(req.items.len());
    for item in &req.items {
        let outcome = match req.mode {
            DeleteMode::RecycleBin => match deleter.recycle(&item.path) {
                Ok(()) => ItemOutcome::Removed,
                Err(reason) => ItemOutcome::Failed(Failure {
                    path: item.path.clone(),
                    reason,
                }),
            },
            DeleteMode::Permanent => deleter.delete_permanently(&item.path),
        };
        out.push((item.id, outcome));
        progress.done.fetch_add(1, Ordering::SeqCst);
    }
    out
}

/// Dossier partiellement supprimé, à rescanner.
#[derive(Clone, Debug)]
pub struct Rescan {
    pub node: NodeId,
    pub path: PathBuf,
    pub old_size: u64,
}

#[derive(Default, Debug)]
pub struct Applied {
    pub freed: u64,
    pub failures: Vec<Failure>,
    pub rescans: Vec<Rescan>,
    pub removed: Vec<NodeId>,
}

/// Répercute les résultats de suppression sur l'arbre.
pub fn apply_outcomes(
    tree: &mut Tree,
    mode: DeleteMode,
    outcomes: &[(NodeId, ItemOutcome)],
) -> Applied {
    let mut applied = Applied::default();
    for (id, outcome) in outcomes {
        let id = *id;
        match outcome {
            ItemOutcome::Failed(f) => applied.failures.push(f.clone()),
            ItemOutcome::Removed => {
                // Mis à la corbeille sur un arbre de disque : déplacé sous le
                // bloc `$Recycle.Bin` (il n'y en a qu'à la racine d'un disque).
                let bin = match mode {
                    DeleteMode::RecycleBin => tree.ensure_recycle_bin(),
                    DeleteMode::Permanent => None,
                };
                if let Some(bin) = bin {
                    tree.reparent(id, bin);
                } else {
                    if mode == DeleteMode::Permanent {
                        applied.freed += tree.node(id).size;
                    }
                    tree.detach(id);
                }
                applied.removed.push(id);
            }
            ItemOutcome::Partial(failures) => {
                let (size, parent, newest, name, path) = {
                    let n = tree.node(id);
                    (
                        n.size,
                        n.parent,
                        n.newest,
                        n.name.to_os_string(),
                        tree.path(id),
                    )
                };
                tree.detach(id);
                if let Some(parent) = parent {
                    let ph = tree.add_children(
                        parent,
                        vec![EntryData {
                            name,
                            kind: NodeKind::Dir,
                            size: 0,
                            newest,
                        }],
                    )[0];
                    applied.rescans.push(Rescan {
                        node: ph,
                        path,
                        old_size: size,
                    });
                }
                applied.failures.extend(failures.iter().cloned());
                applied.removed.push(id);
            }
        }
    }
    applied
}

/// Remet la vue sur une position valide si l'élément affiché ou sélectionné a été supprimé.
pub fn reset_view(
    tree: &Tree,
    removed: &[NodeId],
    view_root: NodeId,
    selected: NodeId,
) -> (NodeId, NodeId) {
    let affected = |id: NodeId| removed.iter().any(|&d| id == d || tree.is_ancestor(d, id));
    if affected(view_root) {
        let root = tree.root();
        (root, root)
    } else if affected(selected) {
        (view_root, view_root)
    } else {
        (view_root, selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{EntryData, NodeKind};
    use std::path::PathBuf;
    use std::sync::Mutex;

    struct FakeDeleter {
        recycle_calls: Mutex<Vec<PathBuf>>,
        permanent_calls: Mutex<Vec<PathBuf>>,
    }
    impl FakeDeleter {
        fn new() -> Self {
            FakeDeleter {
                recycle_calls: Mutex::new(Vec::new()),
                permanent_calls: Mutex::new(Vec::new()),
            }
        }
    }
    impl Deleter for FakeDeleter {
        fn recycle(&self, path: &Path) -> Result<(), FailureReason> {
            self.recycle_calls.lock().unwrap().push(path.to_path_buf());
            Ok(())
        }
        fn delete_permanently(&self, path: &Path) -> ItemOutcome {
            self.permanent_calls
                .lock()
                .unwrap()
                .push(path.to_path_buf());
            ItemOutcome::Removed
        }
    }

    fn entry(n: &str, kind: NodeKind, size: u64) -> EntryData {
        EntryData {
            name: n.into(),
            kind,
            size,
            newest: 10,
        }
    }

    /// Root `root_path` { Videos { v 100 } }.
    fn sample(root_path: &str) -> (Tree, NodeId, NodeId) {
        let mut t = Tree::new(PathBuf::from(root_path), 10);
        let root = t.root();
        let videos = t.add_children(root, vec![entry("Videos", NodeKind::Dir, 0)])[0];
        let v = t.add_children(videos, vec![entry("v", NodeKind::File, 100)])[0];
        (t, videos, v)
    }

    fn failure(p: &str) -> Failure {
        Failure {
            path: PathBuf::from(p),
            reason: FailureReason::InUse,
        }
    }

    #[test]
    fn recycled_item_moves_to_bin_on_drive_tree() {
        let (mut t, videos, _) = sample(r"C:\");
        let root = t.root();
        let a = apply_outcomes(
            &mut t,
            DeleteMode::RecycleBin,
            &[(videos, ItemOutcome::Removed)],
        );
        assert_eq!(t.node(root).size, 100);
        assert_eq!(t.node(t.recycle_bin().unwrap()).size, 100);
        assert_eq!(a.freed, 0);
        assert_eq!(a.removed, vec![videos]);
    }

    #[test]
    fn recycled_item_is_detached_on_folder_tree() {
        let (mut t, videos, _) = sample(r"D:\Data");
        let root = t.root();
        apply_outcomes(
            &mut t,
            DeleteMode::RecycleBin,
            &[(videos, ItemOutcome::Removed)],
        );
        assert_eq!(t.node(root).size, 0);
        assert!(!t.is_attached(videos));
    }

    /// A `$Recycle.Bin` folder below a folder root is not the drive's bin:
    /// recycled items are detached, never moved into it.
    #[test]
    fn recycled_item_never_moves_into_a_folder_named_like_the_bin() {
        let (mut t, videos, _) = sample(r"D:\Data");
        let root = t.root();
        let fake = t.add_children(root, vec![entry("$Recycle.Bin", NodeKind::Dir, 0)])[0];
        apply_outcomes(
            &mut t,
            DeleteMode::RecycleBin,
            &[(videos, ItemOutcome::Removed)],
        );
        assert!(!t.is_attached(videos));
        assert_eq!(t.node(fake).size, 0);
        assert_eq!(t.node(root).size, 0);
    }

    #[test]
    fn removed_permanent_item_is_detached() {
        let (mut t, _, v) = sample(r"C:\");
        let a = apply_outcomes(&mut t, DeleteMode::Permanent, &[(v, ItemOutcome::Removed)]);
        assert_eq!(a.freed, 100);
        assert!(!t.is_attached(v));
    }

    #[test]
    fn partial_creates_placeholder_and_rescan() {
        let (mut t, videos, _) = sample(r"C:\");
        let root = t.root();
        let path = t.path(videos);
        let a = apply_outcomes(
            &mut t,
            DeleteMode::Permanent,
            &[(videos, ItemOutcome::Partial(vec![failure(r"C:\Videos\v")]))],
        );
        assert!(!t.is_attached(videos));
        let ph = a.rescans[0].node;
        assert_ne!(ph, videos);
        assert_eq!(t.node(ph).name, t.node(videos).name);
        assert_eq!(t.node(ph).parent, Some(root));
        assert_eq!(t.node(ph).size, 0);
        assert_eq!(t.node(ph).kind, NodeKind::Dir);
        assert_eq!(a.rescans[0].old_size, 100);
        assert_eq!(a.rescans[0].path, path);
        assert_eq!(a.failures.len(), 1);
        assert_eq!(a.removed, vec![videos]);
    }

    #[test]
    fn failed_item_leaves_tree_untouched() {
        let (mut t, videos, _) = sample(r"C:\");
        let root = t.root();
        let (g, s) = (t.generation(), t.node(root).size);
        let a = apply_outcomes(
            &mut t,
            DeleteMode::RecycleBin,
            &[(videos, ItemOutcome::Failed(failure(r"C:\Videos")))],
        );
        assert_eq!(t.generation(), g);
        assert_eq!(t.node(root).size, s);
        assert_eq!(a.failures.len(), 1);
        assert!(a.removed.is_empty());
    }

    #[test]
    fn run_delete_uses_mode_and_counts_progress() {
        let items = (0..3)
            .map(|i| DeleteItem {
                id: NodeId(i),
                path: PathBuf::from(format!(r"C:\x{i}")),
                size: 1,
            })
            .collect();
        let req = DeleteRequest {
            items,
            mode: DeleteMode::RecycleBin,
        };
        let fake = FakeDeleter::new();
        let progress = DeleteProgress::default();
        let out = run_delete(&req, &fake, &progress);
        assert_eq!(out.len(), 3);
        assert_eq!(fake.recycle_calls.lock().unwrap().len(), 3);
        assert!(fake.permanent_calls.lock().unwrap().is_empty());
        assert_eq!(progress.done.load(Ordering::SeqCst), 3);
        assert_eq!(progress.total.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn reset_view_cases() {
        let (mut t, videos, v) = sample(r"C:\");
        let root = t.root();
        let other = t.add_children(root, vec![entry("Other", NodeKind::Dir, 0)])[0];
        assert_eq!(reset_view(&t, &[videos], v, v), (root, root));
        assert_eq!(reset_view(&t, &[videos], other, v), (other, other));
        assert_eq!(reset_view(&t, &[videos], other, other), (other, other));
    }
}

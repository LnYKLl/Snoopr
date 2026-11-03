pub mod fs;

use crate::tree::{EntryData, NodeId, NodeKind, SharedTree};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Compteurs partagés lisibles par l'interface pendant le scan.
pub struct ScanProgress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub denied: AtomicU64,
    pub current: Mutex<PathBuf>,
}

/// Poignée sur un scan en cours.
pub struct ScanHandle {
    pub progress: Arc<ScanProgress>,
    cancel: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    started: Instant,
    elapsed: Arc<AtomicU64>, // nanosecondes, figé à la fin
    thread: JoinHandle<()>,
}

impl ScanHandle {
    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Durée écoulée ; figée une fois le scan terminé.
    pub fn elapsed(&self) -> Duration {
        if self.is_done() {
            Duration::from_nanos(self.elapsed.load(Ordering::Acquire))
        } else {
            self.started.elapsed()
        }
    }

    /// Attend la fin du scan et renvoie sa durée.
    pub fn wait(self) -> Duration {
        let _ = self.thread.join();
        Duration::from_nanos(self.elapsed.load(Ordering::Acquire))
    }
}

/// Marque le scan terminé même en cas de panique.
struct DoneGuard {
    done: Arc<AtomicBool>,
    elapsed: Arc<AtomicU64>,
    started: Instant,
}

impl Drop for DoneGuard {
    fn drop(&mut self) {
        let nanos = u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.elapsed.store(nanos, Ordering::Release);
        self.done.store(true, Ordering::Release);
    }
}

struct Ctx {
    tree: SharedTree,
    progress: Arc<ScanProgress>,
    cancel: Arc<AtomicBool>,
}

/// Lance le scan de `dir` ; le contenu est ajouté sous le nœud `under`.
pub fn start_scan(tree: SharedTree, under: NodeId, dir: PathBuf) -> ScanHandle {
    let progress = Arc::new(ScanProgress {
        files: AtomicU64::new(0),
        dirs: AtomicU64::new(0),
        denied: AtomicU64::new(0),
        current: Mutex::new(dir.clone()),
    });
    let cancel = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let elapsed = Arc::new(AtomicU64::new(0));
    let started = Instant::now();
    let ctx = Arc::new(Ctx {
        tree,
        progress: progress.clone(),
        cancel: cancel.clone(),
    });
    let guard = DoneGuard {
        done: done.clone(),
        elapsed: elapsed.clone(),
        started,
    };
    let thread = std::thread::spawn(move || {
        let _guard = guard;
        let n = std::thread::available_parallelism().map_or(4, |n| n.get());
        let threads = (n * 2).clamp(4, 32);
        let Ok(pool) = rayon::ThreadPoolBuilder::new().num_threads(threads).build() else {
            return;
        };
        pool.scope(|s| visit(s, &ctx, under, dir));
    });
    ScanHandle {
        progress,
        cancel,
        done,
        started,
        elapsed,
        thread,
    }
}

fn visit<'a>(s: &rayon::Scope<'a>, ctx: &'a Arc<Ctx>, id: NodeId, dir: PathBuf) {
    if ctx.cancel.load(Ordering::Relaxed) {
        return;
    }
    let entries = match fs::read_dir_entries(&dir) {
        Ok(e) => e,
        Err(e) => {
            if e.kind() != io::ErrorKind::NotFound {
                ctx.progress.denied.fetch_add(1, Ordering::Relaxed);
            }
            return;
        }
    };
    let kept: Vec<_> = entries.into_iter().filter(|e| !e.is_link).collect();
    let data: Vec<EntryData> = kept
        .iter()
        .map(|e| EntryData {
            name: e.name.clone(),
            kind: if e.is_dir {
                NodeKind::Dir
            } else {
                NodeKind::File
            },
            size: if e.is_dir { 0 } else { e.alloc_size },
            newest: e.modified.max(e.created),
        })
        .collect();
    let ids = ctx.tree.write().unwrap().add_children(id, data);
    let nfiles = kept.iter().filter(|e| !e.is_dir).count() as u64;
    ctx.progress.files.fetch_add(nfiles, Ordering::Relaxed);
    let before = ctx.progress.dirs.fetch_add(1, Ordering::Relaxed);
    if before.is_multiple_of(64)
        && let Ok(mut cur) = ctx.progress.current.lock()
    {
        *cur = dir.clone();
    }
    for (e, child) in kept.iter().zip(ids) {
        if e.is_dir {
            let path = dir.join(&e.name);
            s.spawn(move |s| visit(s, ctx, child, path));
        }
    }
}

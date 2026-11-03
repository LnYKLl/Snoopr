//! Suppression définitive « au mieux » : ne suit jamais un lien ou une jonction
//! et liste précisément ce qui n'a pas pu être supprimé.

use super::{Failure, ItemOutcome, reason_from_io};
use std::fs;
use std::io;
use std::path::Path;

pub fn delete_permanently(path: &Path) -> ItemOutcome {
    let mut failures = Vec::new();
    let is_dir = delete_entry(path, &mut failures);
    if failures.is_empty() {
        ItemOutcome::Removed
    } else if is_dir {
        ItemOutcome::Partial(failures)
    } else {
        ItemOutcome::Failed(failures.remove(0))
    }
}

fn is_not_found(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotFound
}

fn clear_readonly(path: &Path) {
    if let Ok(meta) = fs::symlink_metadata(path) {
        let mut perm = meta.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = fs::set_permissions(path, perm);
        }
    }
}

/// Exécute `op`, et si elle échoue, retire l'attribut lecture seule puis réessaie une fois.
fn with_readonly_retry(path: &Path, op: fn(&Path) -> io::Result<()>) -> io::Result<()> {
    match op(path) {
        Ok(()) => Ok(()),
        Err(e) if is_not_found(&e) => Ok(()),
        Err(_) => {
            clear_readonly(path);
            match op(path) {
                Err(e) if is_not_found(&e) => Ok(()),
                r => r,
            }
        }
    }
}

fn push_failure(failures: &mut Vec<Failure>, path: &Path, e: &io::Error) {
    failures.push(Failure {
        path: path.to_path_buf(),
        reason: reason_from_io(e),
    });
}

/// Supprime une entrée. Retourne `true` si c'est un vrai dossier (non lien).
fn delete_entry(path: &Path, failures: &mut Vec<Failure>) -> bool {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            if !is_not_found(&e) {
                push_failure(failures, path, &e);
            }
            return false;
        }
    };
    let ft = meta.file_type();

    if ft.is_symlink() {
        // Lien ou jonction : on supprime le lien lui-même, sans jamais le traverser.
        let result = if is_dir_link(&meta) {
            with_readonly_retry(path, |p| fs::remove_dir(p))
        } else {
            with_readonly_retry(path, |p| fs::remove_file(p))
        };
        if let Err(e) = result {
            push_failure(failures, path, &e);
        }
        return false;
    }

    if ft.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(rd) => rd,
            Err(e) => {
                if !is_not_found(&e) {
                    push_failure(failures, path, &e);
                }
                return true;
            }
        };
        let before = failures.len();
        for entry in entries {
            match entry {
                Ok(entry) => {
                    delete_entry(&entry.path(), failures);
                }
                Err(e) => {
                    if !is_not_found(&e) {
                        push_failure(failures, path, &e);
                    }
                }
            }
        }
        // Si un enfant a échoué, le dossier ne peut pas être vidé : ce n'est pas une cause racine.
        let children_ok = failures.len() == before;
        let result = if children_ok {
            with_readonly_retry(path, |p| fs::remove_dir(p))
        } else {
            Ok(())
        };
        if let Err(e) = result {
            push_failure(failures, path, &e);
        }
        return true;
    }

    if let Err(e) = with_readonly_retry(path, |p| fs::remove_file(p)) {
        push_failure(failures, path, &e);
    }
    false
}

#[cfg(windows)]
fn is_dir_link(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::FileTypeExt;
    meta.file_type().is_symlink_dir()
}

#[cfg(not(windows))]
fn is_dir_link(_meta: &fs::Metadata) -> bool {
    false
}

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
        // Épingle le dossier (sans FILE_SHARE_DELETE) : impossible de le renommer ou de le
        // remplacer par une jonction pendant que ses enfants sont traités.
        let pin = match pin::pin_dir(path) {
            Ok(pin::Pinned::Dir(pin)) => pin,
            Ok(pin::Pinned::Link) => {
                // Devenu un lien entre-temps : on supprime le lien lui-même.
                if let Err(e) = with_readonly_retry(path, |p| fs::remove_dir(p)) {
                    push_failure(failures, path, &e);
                }
                return false;
            }
            Err(e) => {
                if !is_not_found(&e) {
                    push_failure(failures, path, &e);
                }
                return true;
            }
        };
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
        // Libère le dossier juste avant sa suppression.
        drop(pin);
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

#[cfg(windows)]
mod pin {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GetFileInformationByHandle, OPEN_EXISTING,
    };
    use windows::core::PCWSTR;

    /// Garde RAII : ferme le handle sur tous les chemins (panique comprise).
    pub struct DirPin(HANDLE);

    impl Drop for DirPin {
        fn drop(&mut self) {
            // SAFETY: le handle a été ouvert par `CreateFileW` et n'est fermé qu'ici.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    pub enum Pinned {
        Dir(DirPin),
        Link,
    }

    fn win32_err(e: windows::core::Error) -> io::Error {
        io::Error::from_raw_os_error(e.code().0 & 0xFFFF)
    }

    /// Chemin « verbatim » (préfixe `\?\`) pour supporter les chemins longs.
    fn wide(path: &Path) -> Vec<u16> {
        let text = path.to_string_lossy();
        let p: PathBuf = if !text.starts_with(r"\\?\") && path.is_absolute() {
            let text = text.replace('/', r"\");
            match text.strip_prefix(r"\\") {
                Some(unc) => PathBuf::from(format!(r"\\?\UNC\{unc}")),
                None => PathBuf::from(format!(r"\\?\{text}")),
            }
        } else {
            path.to_path_buf()
        };
        p.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    pub fn pin_dir(path: &Path) -> io::Result<Pinned> {
        let w = wide(path);
        // SAFETY: `w` est une chaîne UTF-16 terminée par un NUL qui vit pendant l'appel.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(w.as_ptr()),
                FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
        }
        .map_err(win32_err)?;
        let guard = DirPin(handle);
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: handle valide détenu par `guard`, `info` est une structure valide en écriture.
        unsafe { GetFileInformationByHandle(guard.0, &mut info) }.map_err(win32_err)?;
        let attrs = info.dwFileAttributes;
        if attrs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 || attrs & FILE_ATTRIBUTE_DIRECTORY.0 == 0 {
            return Ok(Pinned::Link);
        }
        Ok(Pinned::Dir(guard))
    }
}

#[cfg(not(windows))]
mod pin {
    use std::io;
    use std::path::Path;

    pub struct DirPin;

    pub enum Pinned {
        Dir(DirPin),
        Link,
    }

    pub fn pin_dir(_path: &Path) -> io::Result<Pinned> {
        Ok(Pinned::Dir(DirPin))
    }
}

//! Fixed drives, their usage, and the "Not scanned" block of a drive scan.

use std::io;
use std::path::{Path, PathBuf};

use crate::tree::Tree;

#[derive(Clone, Debug)]
pub struct Volume {
    pub root: PathBuf,
    pub label: String,
    pub total: u64,
    pub free: u64,
}

/// Strips a verbatim prefix and normalizes separators to a plain string.
fn plain(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\");
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s
    }
}

fn drive_letter_prefix(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Byte length of the `servershare` part of a UNC path, if it is one.
fn unc_root_len(s: &str) -> Option<usize> {
    let rest = s.strip_prefix(r"\\")?;
    let mut parts = rest.splitn(3, '\\');
    let server = parts.next().filter(|p| !p.is_empty())?;
    let share = parts.next().filter(|p| !p.is_empty())?;
    Some(2 + server.len() + 1 + share.len())
}

/// Maps a path to its volume root (drive letter or UNC share); other inputs are returned unchanged.
pub fn volume_root_of(path: &Path) -> PathBuf {
    let s = plain(path);
    if drive_letter_prefix(&s) {
        return PathBuf::from(format!("{}:\\", &s[..1]));
    }
    if let Some(len) = unc_root_len(&s) {
        return PathBuf::from(format!("{}\\", &s[..len]));
    }
    path.to_path_buf()
}

pub fn is_drive_root(path: &Path) -> bool {
    let s = plain(path);
    if drive_letter_prefix(&s) {
        return s.len() == 2 || (s.len() == 3 && s.ends_with('\\'));
    }
    if let Some(len) = unc_root_len(&s) {
        return s.len() == len || (s.len() == len + 1 && s.ends_with('\\'));
    }
    false
}

/// Sets the "Not scanned" block at the root of a drive scan:
/// `(total - free)` minus what was scanned. Does nothing for non-drive roots.
pub fn update_unscanned(tree: &mut Tree, total: u64, free: u64) {
    if !is_drive_root(tree.root_path()) {
        return;
    }
    let used = total.saturating_sub(free);
    tree.set_unscanned(used.saturating_sub(tree.scanned_size()));
}

#[cfg(windows)]
mod sys {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };
    use windows::core::PCWSTR;

    const DRIVE_FIXED: u32 = 3;

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    pub fn volume_usage(path: &Path) -> io::Result<(u64, u64)> {
        let w = wide(path);
        let (mut total, mut free) = (0u64, 0u64);
        // SAFETY: `w` is NUL-terminated and outlives the call; out pointers are valid.
        unsafe { GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), Some(&mut free), Some(&mut total), None) }
            .map_err(|e| io::Error::from_raw_os_error(e.code().0 & 0xFFFF))?;
        Ok((total, free))
    }

    fn label_of(root: &Path) -> String {
        let w = wide(root);
        let mut buf = [0u16; 261];
        // SAFETY: valid NUL-terminated input and a writable buffer.
        let ok = unsafe {
            GetVolumeInformationW(PCWSTR(w.as_ptr()), Some(&mut buf), None, None, None, None)
        };
        if ok.is_err() {
            return String::new();
        }
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..n])
    }

    pub fn list_fixed_volumes() -> Vec<Volume> {
        // SAFETY: no arguments.
        let mask = unsafe { GetLogicalDrives() };
        let mut out = Vec::new();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let root = PathBuf::from(format!("{}:\\", (b'A' + i as u8) as char));
            let w = wide(&root);
            // SAFETY: valid NUL-terminated input.
            if unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) } != DRIVE_FIXED {
                continue;
            }
            let Ok((total, free)) = volume_usage(&root) else {
                continue;
            };
            let label = label_of(&root);
            out.push(Volume {
                root,
                label,
                total,
                free,
            });
        }
        out
    }
}

#[cfg(windows)]
pub use sys::{list_fixed_volumes, volume_usage};

#[cfg(not(windows))]
pub fn list_fixed_volumes() -> Vec<Volume> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn volume_usage(_path: &Path) -> io::Result<(u64, u64)> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{EntryData, NodeKind};

    fn file_tree(root: &str, bytes: u64) -> Tree {
        let mut t = Tree::new(PathBuf::from(root), 0);
        let r = t.root();
        t.add_children(
            r,
            vec![EntryData {
                name: "f".into(),
                kind: NodeKind::File,
                size: bytes,
                newest: 0,
            }],
        );
        t
    }

    fn unscanned_size(t: &Tree) -> u64 {
        t.node(t.root())
            .children
            .iter()
            .map(|&c| t.node(c))
            .filter(|n| n.kind == NodeKind::Unscanned)
            .map(|n| n.size)
            .sum()
    }

    fn unscanned_count(t: &Tree) -> usize {
        t.node(t.root())
            .children
            .iter()
            .filter(|&&c| t.node(c).kind == NodeKind::Unscanned)
            .count()
    }

    #[test]
    fn roots() {
        assert_eq!(volume_root_of(Path::new(r"C:\x\y")), PathBuf::from(r"C:\"));
        assert_eq!(volume_root_of(Path::new(r"D:\")), PathBuf::from(r"D:\"));
        assert_eq!(
            volume_root_of(Path::new(r"\\?\C:\x")),
            PathBuf::from(r"C:\")
        );
        assert_eq!(
            volume_root_of(Path::new(r"\\srv\share\a\b")),
            PathBuf::from(r"\\srv\share\")
        );
        assert!(is_drive_root(Path::new(r"C:\")));
        assert!(is_drive_root(Path::new(r"\\?\C:\")));
        assert!(is_drive_root(Path::new(r"\\srv\share")));
        assert!(!is_drive_root(Path::new(r"C:\Users")));
        assert!(!is_drive_root(Path::new(r"\\srv\share\x")));
    }

    #[test]
    fn update_unscanned_math() {
        let mut t = file_tree(r"C:\", 100);
        update_unscanned(&mut t, 1000, 800);
        assert_eq!(unscanned_size(&t), 100);

        let mut t = file_tree(r"C:\", 100);
        update_unscanned(&mut t, 1000, 950);
        assert_eq!(unscanned_size(&t), 0);

        let mut t = file_tree(r"D:\Data", 100);
        update_unscanned(&mut t, 1000, 800);
        assert_eq!(unscanned_count(&t), 0);
    }

    #[cfg(windows)]
    #[test]
    fn lists_system_drive() {
        let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let root = volume_root_of(Path::new(&format!("{sys}\\")));
        let vols = list_fixed_volumes();
        let v = vols.iter().find(|v| v.root == root).expect("system drive");
        assert!(v.total > 0);
    }
}

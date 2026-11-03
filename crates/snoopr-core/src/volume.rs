//! Fixed drives, their usage, the real path of a scan target, and the
//! "Not scanned" block of a drive scan.

use std::io;
use std::path::{Path, PathBuf};

use crate::tree::Tree;

/// The real path of a scan target, as Windows resolves it (the final path of
/// an open handle): SUBST drives, junctions, 8.3 names, `.` and `..`, `/`
/// separators and relative paths are resolved, then the verbatim prefix is
/// dropped (see [`without_verbatim`]). The safety rules read paths as text, so
/// they must only ever see this form. `None` when the path cannot be resolved,
/// for instance because it does not exist.
pub fn canonical_target(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path)
        .ok()
        .map(|p| without_verbatim(&p))
}

/// `\\?\C:\x` -> `C:\x` and `\\?\UNC\srv\share\x` -> `\\srv\share\x`, as the
/// `dunce` crate does. Anything else is returned as is: the other verbatim
/// forms (`\\?\Volume{…}\x`), and paths whose plain form would name something
/// else (a `.` or `..` part, a name ending in a dot or a space, a DOS device
/// name such as `nul`) or would be too long for it.
#[cfg(windows)]
pub fn without_verbatim(path: &Path) -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Component, Prefix};

    let w: Vec<u16> = path.as_os_str().encode_wide().collect();
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return path.to_path_buf();
    };
    // The prefix parser reports these two kinds only for an exact `\\?\C:`
    // or `\\?\UNC\` written with backslashes.
    let (plain, names): (Vec<u16>, &[u16]) = match prefix.kind() {
        // `\\?\C:\…`; a bare `\\?\C:` names the volume, not its root.
        Prefix::VerbatimDisk(_) if w.get(6) == Some(&BACKSLASH) => (w[4..].to_vec(), &w[7..]),
        Prefix::VerbatimUNC(..) => {
            let plain = [BACKSLASH; 2].into_iter().chain(w[8..].iter().copied());
            (plain.collect(), &w[8..])
        }
        _ => return path.to_path_buf(),
    };
    // MAX_PATH counts the final NUL.
    if plain.len() >= 260 || !plain_names_are_safe(names) {
        return path.to_path_buf();
    }
    PathBuf::from(OsString::from_wide(&plain))
}

#[cfg(windows)]
const BACKSLASH: u16 = b'\\' as u16;

#[cfg(not(windows))]
pub fn without_verbatim(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// The names of `names` (separated by `\`, with at most one trailing `\`)
/// mean the same thing without the verbatim prefix.
#[cfg(windows)]
fn plain_names_are_safe(names: &[u16]) -> bool {
    let names = names.strip_suffix(&[BACKSLASH]).unwrap_or(names);
    if names.is_empty() {
        return true;
    }
    let bad_char = |c: &u16| *c < 32 || br#"<>:"/|?*"#.iter().any(|&b| *c == u16::from(b));
    names.split(|&c| c == BACKSLASH).all(|name| {
        let last = name.last().copied();
        !name.is_empty()
            && !name.iter().any(bad_char)
            && last != Some(u16::from(b'.'))
            && last != Some(u16::from(b' '))
            && !is_dos_device(name)
    })
}

/// `con`, `nul.txt`, `COM1`…: names that Win32 maps to a device.
#[cfg(windows)]
fn is_dos_device(name: &[u16]) -> bool {
    let stem = name.split(|&c| c == u16::from(b'.')).next().unwrap_or(name);
    let stem = String::from_utf16_lossy(stem);
    let stem = stem.trim_end_matches(' ').to_ascii_uppercase();
    let numbered = |p: &str| {
        stem.strip_prefix(p)
            .is_some_and(|n| n.chars().count() == 1 && "123456789¹²³".contains(n))
    };
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || numbered("COM")
        || numbered("LPT")
}

/// `QueryDosDeviceW("X:")` of a SUBST letter names a folder (`\??\C:\…`);
/// that of a real volume names a device (`\Device\HarddiskVolume3`).
#[cfg_attr(not(windows), allow(dead_code))]
fn is_subst_target(target: &str) -> bool {
    target.starts_with(r"\??\")
}

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
        QueryDosDeviceW,
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

    /// Whether `letter:` is a SUBST drive. Windows reports those as fixed
    /// drives, but they only alias a folder of another drive. A letter that
    /// cannot be queried is kept: a scan resolves its target anyway.
    fn is_subst(letter: char) -> bool {
        let name: Vec<u16> = format!("{letter}:").encode_utf16().chain(Some(0)).collect();
        let mut buf = vec![0u16; 32 * 1024];
        // SAFETY: NUL-terminated input; the output slice is writable and its
        // length is passed with it.
        let n = unsafe { QueryDosDeviceW(PCWSTR(name.as_ptr()), Some(&mut buf)) } as usize;
        // The first string of the returned list is the current mapping.
        let first = buf[..n.min(buf.len())]
            .split(|&c| c == 0)
            .next()
            .unwrap_or(&[]);
        n > 0 && is_subst_target(&String::from_utf16_lossy(first))
    }

    pub fn list_fixed_volumes() -> Vec<Volume> {
        // SAFETY: no arguments.
        let mask = unsafe { GetLogicalDrives() };
        let mut out = Vec::new();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let root = PathBuf::from(format!("{letter}:\\"));
            let w = wide(&root);
            // SAFETY: valid NUL-terminated input.
            if unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) } != DRIVE_FIXED || is_subst(letter) {
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
    fn without_verbatim_forms() {
        let same = |p: &str| assert_eq!(without_verbatim(Path::new(p)), PathBuf::from(p), "{p}");
        let plain = |p: &str, want: &str| {
            assert_eq!(without_verbatim(Path::new(p)), PathBuf::from(want), "{p}");
        };
        plain(r"\\?\C:\x", r"C:\x");
        plain(r"\\?\C:\", r"C:\");
        plain(r"\\?\d:\Data\Vidéos", r"d:\Data\Vidéos");
        plain(r"\\?\UNC\srv\share\x", r"\\srv\share\x");
        plain(r"\\?\UNC\srv\share", r"\\srv\share");
        same(r"\\?\Volume{0b1c2d3e-4f50-6172-8394-a5b6c7d8e9f0}\x");
        same(r"\\?\GLOBALROOT\Device\HarddiskVolume3\x");
        // Kept verbatim when the plain form would name something else.
        same(r"\\?\C:\x\trailing.");
        same(r"\\?\C:\x\space ");
        same(r"\\?\C:\x\nul");
        same(r"\\?\C:\x\COM1.txt");
        same(r"\\?\C:\x\..\y");
        same(r"\\?\C:\x\\y");
        same(&format!(r"\\?\C:\{}", "a".repeat(300)));
        // Already plain.
        same(r"C:\x");
        same(r"\\srv\share\x");
    }

    #[test]
    fn subst_targets_are_recognized() {
        assert!(is_subst_target(r"\??\C:\Users\lenny\Data"));
        assert!(!is_subst_target(r"\Device\HarddiskVolume3"));
        assert!(!is_subst_target(""));
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

//! Énumération bas niveau d'un dossier : taille allouée, dates et détection des liens en un passage.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

/// Une entrée brute d'un dossier (sans `.` ni `..`).
#[derive(Clone, Debug)]
pub struct RawEntry {
    pub name: OsString,
    pub is_dir: bool,
    /// Jonction ou lien symbolique uniquement (pas les placeholders cloud).
    pub is_link: bool,
    pub alloc_size: u64,
    /// Secondes Unix.
    pub modified: i64,
    /// Secondes Unix.
    pub created: i64,
}

/// `C:\a` -> `\\?\C:\a` ; `\\srv\share\x` -> `\\?\UNC\srv\share\x` ; déjà verbatim -> inchangé.
pub fn to_verbatim(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    if s.starts_with(r"\\?\") || s.starts_with(r"\\.\") {
        path.to_path_buf()
    } else if let Some(rest) = s.strip_prefix(r"\\") {
        PathBuf::from(format!(r"\\?\UNC\{rest}"))
    } else {
        PathBuf::from(format!(r"\\?\{s}"))
    }
}

#[cfg(windows)]
pub fn read_dir_entries(dir: &Path) -> io::Result<Vec<RawEntry>> {
    win::read_dir_entries(dir)
}

#[cfg(windows)]
mod win {
    use super::{RawEntry, to_verbatim};
    use std::ffi::OsString;
    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::Path;
    use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FULL_DIR_INFO, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FileFullDirectoryInfo, FileFullDirectoryRestartInfo,
        GetFileInformationByHandleEx, OPEN_EXISTING,
    };
    use windows::core::PCWSTR;

    const TAG_MOUNT_POINT: u32 = 0xA000_0003;
    const TAG_SYMLINK: u32 = 0xA000_000C;
    const BUF_BYTES: usize = 64 * 1024;

    /// Ferme le handle à la sortie de portée.
    struct Guard(HANDLE);
    impl Drop for Guard {
        fn drop(&mut self) {
            // SAFETY: le handle a été ouvert par CreateFileW et n'est fermé qu'ici.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    fn filetime_to_unix(ft: i64) -> i64 {
        ft / 10_000_000 - 11_644_473_600
    }

    /// Code Win32 d'une erreur `windows` (HRESULT FACILITY_WIN32 -> code brut).
    fn win32_code(e: &windows::core::Error) -> i32 {
        let hr = e.code().0 as u32;
        if hr >> 16 == 0x8007 {
            (hr & 0xFFFF) as i32
        } else {
            hr as i32
        }
    }

    pub fn read_dir_entries(dir: &Path) -> io::Result<Vec<RawEntry>> {
        let wide: Vec<u16> = to_verbatim(dir)
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `wide` est terminé par un NUL et vit pendant l'appel.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_LIST_DIRECTORY.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )
        }
        .map_err(|e| io::Error::from_raw_os_error(win32_code(&e)))?;
        let guard = Guard(handle);

        // u64 pour garantir l'alignement sur 8 octets de FILE_FULL_DIR_INFO.
        let mut buf = vec![0u64; BUF_BYTES / 8];
        let mut out = Vec::new();
        let mut first = true;
        loop {
            let class = if first {
                FileFullDirectoryRestartInfo
            } else {
                FileFullDirectoryInfo
            };
            first = false;
            // SAFETY: le tampon fait BUF_BYTES octets et reste valide pendant l'appel.
            let res = unsafe {
                GetFileInformationByHandleEx(
                    guard.0,
                    class,
                    buf.as_mut_ptr().cast(),
                    BUF_BYTES as u32,
                )
            };
            if let Err(e) = res {
                let code = win32_code(&e);
                if code == ERROR_NO_MORE_FILES.0 as i32 {
                    break;
                }
                return Err(io::Error::from_raw_os_error(code));
            }
            // SAFETY: le tampon u64 est valide sur BUF_BYTES octets, vu comme octets.
            let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), BUF_BYTES) };
            parse_chain(bytes, &mut out);
        }
        Ok(out)
    }

    /// Parcourt la chaîne d'enregistrements, en vérifiant chaque borne dans `bytes`.
    fn parse_chain(bytes: &[u8], out: &mut Vec<RawEntry>) {
        let name_off = std::mem::offset_of!(FILE_FULL_DIR_INFO, FileName);
        let mut off = 0usize;
        loop {
            if off + name_off > bytes.len() {
                return;
            }
            // SAFETY: off + name_off <= len ; lecture non alignée d'une struct POD
            // dont la partie fixe tient dans les octets vérifiés.
            let info: FILE_FULL_DIR_INFO = unsafe {
                std::ptr::read_unaligned(bytes.as_ptr().add(off).cast::<FILE_FULL_DIR_INFO>())
            };
            let name_len = info.FileNameLength as usize;
            let name_start = off + name_off;
            if !name_len.is_multiple_of(2) || name_start + name_len > bytes.len() {
                return;
            }
            let units: Vec<u16> = bytes[name_start..name_start + name_len]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let name = OsString::from_wide(&units);
            if name != "." && name != ".." {
                let attrs = info.FileAttributes;
                let reparse = attrs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0;
                out.push(RawEntry {
                    name,
                    is_dir: attrs & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
                    is_link: reparse && matches!(info.EaSize, TAG_MOUNT_POINT | TAG_SYMLINK),
                    alloc_size: info.AllocationSize.max(0) as u64,
                    modified: filetime_to_unix(info.LastWriteTime),
                    created: filetime_to_unix(info.CreationTime),
                });
            }
            let next = info.NextEntryOffset as usize;
            if next == 0 {
                return;
            }
            off += next;
        }
    }
}

#[cfg(not(windows))]
pub fn read_dir_entries(dir: &Path) -> io::Result<Vec<RawEntry>> {
    use std::time::UNIX_EPOCH;
    let secs = |t: io::Result<std::time::SystemTime>| {
        t.ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64)
    };
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let md = std::fs::symlink_metadata(e.path())?;
        out.push(RawEntry {
            name: e.file_name(),
            is_dir: md.is_dir(),
            is_link: md.file_type().is_symlink(),
            alloc_size: md.len(),
            modified: secs(md.modified()),
            created: secs(md.created()),
        });
    }
    Ok(out)
}

//! Corbeille Windows : capacité, envoi (IFileOperation avec filet de sécurité), requête et vidage.

use crate::delete::FailureReason;
use std::io;
use std::path::Path;

/// Capacité de la corbeille pour un volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecycleCapacity {
    Disabled,
    MaxBytes(u64),
    Unknown,
}

/// Un élément de `size` octets tient-il dans la corbeille ?
pub fn fits_in_recycle_bin(size: u64, cap: RecycleCapacity) -> bool {
    match cap {
        RecycleCapacity::Disabled => false,
        RecycleCapacity::MaxBytes(m) => size <= m,
        RecycleCapacity::Unknown => true,
    }
}

/// La corbeille est vide seulement s'il n'y a aucun élément (la taille peut être 0 avec des éléments).
#[cfg_attr(not(windows), allow(dead_code))]
fn bin_is_empty(_size: i64, items: i64) -> bool {
    items == 0
}

/// Convertit un HRESULT en raison d'échec.
pub fn reason_from_hresult(hr: i32) -> FailureReason {
    match hr as u32 {
        0x8007_0020 | 0x8007_0021 => FailureReason::InUse,
        0x8007_0005 => FailureReason::AccessDenied,
        0x8007_04C7 => FailureReason::Cancelled,
        other => FailureReason::Other(format!("HRESULT 0x{other:08X}")),
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{S_FALSE, S_OK};
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumeNameForVolumeMountPointW};
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    };
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    use windows::Win32::UI::Shell::{
        FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING,
        FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName,
        SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHEmptyRecycleBinW, SHQUERYRBINFO,
        SHQueryRecycleBinW,
    };
    use windows::core::PCWSTR;

    const DRIVE_FIXED: u32 = 3;

    fn wide_nul(s: impl Iterator<Item = u16>) -> Vec<u16> {
        s.chain(std::iter::once(0)).collect()
    }

    /// Racine terminée par un antislash, NUL-terminée, pour les API de volume.
    fn root_wide(root: &Path) -> Vec<u16> {
        let mut v: Vec<u16> = root.as_os_str().encode_wide().collect();
        if v.last() != Some(&(b'\\' as u16)) {
            v.push(b'\\' as u16);
        }
        wide_nul(v.into_iter())
    }

    fn read_dword(subkey: &[u16], name: &str) -> Option<u32> {
        let name = wide_nul(name.encode_utf16());
        let mut data = 0u32;
        let mut size = 4u32;
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_DWORD,
                None,
                Some(&mut data as *mut u32 as *mut _),
                Some(&mut size),
            )
        };
        rc.is_ok().then_some(data)
    }

    pub fn recycle_capacity(volume_root: &Path) -> RecycleCapacity {
        let root = root_wide(volume_root);
        if unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) } != DRIVE_FIXED {
            return RecycleCapacity::Disabled;
        }
        let mut buf = [0u16; 64];
        if unsafe { GetVolumeNameForVolumeMountPointW(PCWSTR(root.as_ptr()), &mut buf) }.is_err() {
            return RecycleCapacity::Unknown;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let name = String::from_utf16_lossy(&buf[..len]);
        let (Some(a), Some(b)) = (name.find('{'), name.find('}')) else {
            return RecycleCapacity::Unknown;
        };
        if b < a {
            return RecycleCapacity::Unknown;
        }
        let key = format!(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket\Volume\{}",
            &name[a..=b]
        );
        let key = wide_nul(key.encode_utf16());
        if read_dword(&key, "NukeOnDelete") == Some(1) {
            return RecycleCapacity::Disabled;
        }
        match read_dword(&key, "MaxCapacity") {
            Some(mb) => RecycleCapacity::MaxBytes(mb as u64 * 1_048_576),
            None => RecycleCapacity::Unknown,
        }
    }

    /// Initialise COM sur le thread appelant. `S_FALSE` et `RPC_E_CHANGED_MODE`
    /// sont tolérés ; on ne désinitialise que si notre appel a réussi.
    struct ComGuard(bool);
    impl ComGuard {
        fn new() -> Self {
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            ComGuard(hr == S_OK || hr == S_FALSE)
        }
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// `Ok(true)` si Windows a signalé une opération annulée.
    fn do_recycle(path: &Path) -> windows::core::Result<bool> {
        unsafe {
            let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)?;
            // FOF_WANTNUKEWARNING : filet de sécurité, Windows demande avant
            // toute destruction définitive non prévue.
            op.SetOperationFlags(
                FOF_ALLOWUNDO
                    | FOF_NOCONFIRMATION
                    | FOF_SILENT
                    | FOF_NOERRORUI
                    | FOF_WANTNUKEWARNING,
            )?;
            let w = wide_nul(path.as_os_str().encode_wide());
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None)?;
            op.DeleteItem(&item, None)?;
            op.PerformOperations()?;
            Ok(op.GetAnyOperationsAborted()?.as_bool())
        }
    }

    pub fn recycle(path: &Path) -> Result<(), FailureReason> {
        let _com = ComGuard::new();
        let res = do_recycle(path);
        if matches!(res, Ok(true)) {
            return Err(FailureReason::Cancelled);
        }
        if std::fs::symlink_metadata(path).is_err() {
            return Ok(());
        }
        match res {
            Err(e) => Err(reason_from_hresult(e.code().0)),
            Ok(_) => Err(FailureReason::Other(
                "item still exists after recycle".into(),
            )),
        }
    }

    fn query_size_and_items(volume_root: &Path) -> io::Result<(i64, i64)> {
        let root = root_wide(volume_root);
        let mut info = SHQUERYRBINFO {
            cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
            ..Default::default()
        };
        unsafe { SHQueryRecycleBinW(PCWSTR(root.as_ptr()), &mut info) }
            .map_err(|e| io::Error::from_raw_os_error(e.code().0))?;
        Ok((info.i64Size, info.i64NumItems))
    }

    pub fn query_recycle_bin(volume_root: &Path) -> io::Result<u64> {
        Ok(query_size_and_items(volume_root)?.0.max(0) as u64)
    }

    pub fn empty_recycle_bin(volume_root: &Path) -> io::Result<()> {
        // Comme `recycle` : appelable depuis un thread de travail sans COM.
        let _com = ComGuard::new();
        let root = root_wide(volume_root);
        let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
        // Une corbeille déjà vide est un succès : on vérifie d'abord.
        if query_size_and_items(volume_root).is_ok_and(|(size, items)| bin_is_empty(size, items)) {
            return Ok(());
        }
        unsafe { SHEmptyRecycleBinW(None, PCWSTR(root.as_ptr()), flags) }
            .map_err(|e| io::Error::from_raw_os_error(e.code().0))
    }
}

#[cfg(windows)]
pub use imp::{empty_recycle_bin, query_recycle_bin, recycle, recycle_capacity};

#[cfg(not(windows))]
mod stub {
    use super::*;
    pub fn recycle_capacity(_volume_root: &Path) -> RecycleCapacity {
        RecycleCapacity::Unknown
    }
    pub fn recycle(_path: &Path) -> Result<(), FailureReason> {
        Err(FailureReason::Other("unsupported platform".into()))
    }
    pub fn query_recycle_bin(_volume_root: &Path) -> io::Result<u64> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
    pub fn empty_recycle_bin(_volume_root: &Path) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}
#[cfg(not(windows))]
pub use stub::{empty_recycle_bin, query_recycle_bin, recycle, recycle_capacity};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn fits_table() {
        assert!(!fits_in_recycle_bin(10, RecycleCapacity::Disabled));
        assert!(fits_in_recycle_bin(10, RecycleCapacity::MaxBytes(10)));
        assert!(!fits_in_recycle_bin(11, RecycleCapacity::MaxBytes(10)));
        assert!(fits_in_recycle_bin(u64::MAX, RecycleCapacity::Unknown));
    }

    #[test]
    fn bin_is_empty_uses_item_count() {
        assert!(bin_is_empty(0, 0));
        assert!(!bin_is_empty(0, 3));
        assert!(!bin_is_empty(10, 1));
    }

    #[test]
    fn hresult_table() {
        use crate::delete::FailureReason as F;
        assert_eq!(reason_from_hresult(0x8007_0020u32 as i32), F::InUse);
        assert_eq!(reason_from_hresult(0x8007_0021u32 as i32), F::InUse);
        assert_eq!(reason_from_hresult(0x8007_0005u32 as i32), F::AccessDenied);
        assert_eq!(reason_from_hresult(0x8007_04C7u32 as i32), F::Cancelled);
        assert!(matches!(
            reason_from_hresult(0x8000_4005u32 as i32),
            F::Other(_)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn capacity_and_query_do_not_panic() {
        let _ = recycle_capacity(Path::new(r"C:\"));
        assert!(query_recycle_bin(Path::new(r"C:\")).is_ok());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "manuel : pollue la vraie corbeille"]
    fn recycles_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("snoopr_recycle_test.txt");
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(recycle(&f), Ok(()));
        assert!(!f.exists());
    }
}

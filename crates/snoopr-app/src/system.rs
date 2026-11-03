//! Windows integration: Explorer, elevation query and relaunch as administrator.

use std::io;
use std::path::Path;

/// Opens Explorer on `path`; with `select`, on its parent with `path` selected.
/// Fire-and-forget: the child is not waited for.
#[cfg(windows)]
pub fn open_in_explorer(path: &Path, select: bool) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("explorer.exe");
    if select {
        cmd.raw_arg(format!("/select,\"{}\"", path.display()));
    } else {
        cmd.arg(path);
    }
    cmd.spawn().map(drop)
}

#[cfg(not(windows))]
pub fn open_in_explorer(_path: &Path, _select: bool) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

/// True when the process token is elevated (running as administrator).
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    // SAFETY: plain Win32 calls; the token handle is closed before returning and
    // the output buffer is a properly sized, aligned TOKEN_ELEVATION.
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(std::ptr::from_mut(&mut elevation).cast()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// Restarts this executable elevated, scanning `target`. `Ok` once the new
/// process was launched; an `Err` includes the user declining the UAC prompt.
#[cfg(windows)]
pub fn relaunch_as_admin(target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{PCWSTR, w};
    let wide = |s: &std::ffi::OsStr| -> Vec<u16> { s.encode_wide().chain(Some(0)).collect() };
    let exe = std::env::current_exe()?;
    let exe_w = wide(exe.as_os_str());
    let mut args = std::ffi::OsString::from("\"");
    args.push(target.as_os_str());
    args.push("\"");
    let args_w = wide(&args);
    // SAFETY: both buffers are NUL-terminated and outlive the call.
    let h = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR(args_w.as_ptr()),
            None,
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success (legacy ShellExecute contract).
    if h.0 as isize > 32 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(windows))]
pub fn relaunch_as_admin(_target: &Path) -> io::Result<()> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn elevation_query_does_not_panic() {
        let _ = is_elevated();
    }
}

//! Windows integration: Explorer, elevation query, relaunch as administrator,
//! the double-click time and the animations setting.

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

/// The longest gap Windows allows between the two clicks of a double-click
/// (user setting, 500 ms by default).
#[cfg(windows)]
pub fn double_click_time() -> Option<std::time::Duration> {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
    // SAFETY: no arguments; it only reads a user setting.
    let ms = unsafe { GetDoubleClickTime() };
    Some(std::time::Duration::from_millis(u64::from(ms)))
}

#[cfg(not(windows))]
pub fn double_click_time() -> Option<std::time::Duration> {
    None
}

/// Windows' "Show animations in Windows" (`SPI_GETCLIENTAREAANIMATION`).
/// True when the query fails.
#[cfg(windows)]
pub fn animations_enabled() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    use windows::core::BOOL;
    let mut on = BOOL(1);
    // SAFETY: this action writes one BOOL through the pointer, which outlives the call.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(std::ptr::from_mut(&mut on).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok();
    !ok || on.as_bool()
}

#[cfg(not(windows))]
pub fn animations_enabled() -> bool {
    true
}

/// Quotes one command-line argument (wide units, lossless) following the MSVC
/// argv rules: backslashes before a `"` or before the closing quote are doubled
/// and embedded quotes are escaped, so a root like `C:\` is passed as `"C:\\"`.
#[cfg_attr(not(windows), allow(dead_code))]
fn quote_arg(arg: &[u16]) -> Vec<u16> {
    const QUOTE: u16 = b'"' as u16;
    const BACKSLASH: u16 = b'\\' as u16;
    let mut out = vec![QUOTE];
    let mut backslashes = 0;
    for &c in arg {
        match c {
            BACKSLASH => backslashes += 1,
            QUOTE => {
                out.extend(std::iter::repeat_n(BACKSLASH, backslashes * 2 + 1));
                out.push(QUOTE);
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n(BACKSLASH, backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n(BACKSLASH, backslashes * 2));
    out.push(QUOTE);
    out
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
    let mut args_w = quote_arg(&target.as_os_str().encode_wide().collect::<Vec<u16>>());
    args_w.push(0);
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

    fn q(s: &str) -> String {
        let w: Vec<u16> = s.encode_utf16().collect();
        String::from_utf16(&quote_arg(&w)).unwrap()
    }

    /// Minimal MSVC argv parser for one quoted argument.
    fn parse_one(s: &str) -> String {
        let c: Vec<char> = s.chars().collect();
        assert_eq!(c[0], '"');
        let (mut out, mut bs) = (String::new(), 0);
        for &ch in &c[1..] {
            match ch {
                '\\' => bs += 1,
                '"' => {
                    out.extend(std::iter::repeat_n('\\', bs / 2));
                    if bs % 2 == 0 {
                        return out;
                    }
                    out.push('"');
                    bs = 0;
                }
                ch => {
                    out.extend(std::iter::repeat_n('\\', bs));
                    out.push(ch);
                    bs = 0;
                }
            }
        }
        panic!("unterminated");
    }

    #[test]
    fn quote_arg_escapes_trailing_backslashes() {
        assert_eq!(q(r"C:\"), r#""C:\\""#);
        assert_eq!(q(r"C:\Users\Lenny"), r#""C:\Users\Lenny""#);
        assert_eq!(q(r"D:\My Data\"), r#""D:\My Data\\""#);
        assert_eq!(q(r"X:\a\\"), r#""X:\a\\\\""#);
        assert_eq!(q(r#"a"b\"#), r#""a\"b\\""#);
        for s in [
            r"C:\",
            r"C:\Users\Lenny",
            r"D:\My Data\",
            r"X:\a\\",
            r#"a"b\"#,
            r#"a\"b"#,
        ] {
            assert_eq!(parse_one(&q(s)), s);
        }
    }

    #[cfg(windows)]
    #[test]
    fn elevation_query_does_not_panic() {
        let _ = is_elevated();
    }

    #[cfg(windows)]
    #[test]
    fn animations_query_does_not_panic() {
        let _ = animations_enabled();
    }
}

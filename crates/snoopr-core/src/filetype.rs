//! Content-type classification used to colour the treemap: a context
//! inherited from parent folders, then file extensions.

use crate::rules::{Block, RuleContext, Verdict, Warning, classify};
use std::ffi::OsStr;
use std::path::Path;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileType {
    Game = 0,
    Media = 1,
    Archive = 2,
    VirtualDisk = 3,
    Cache = 4,
    Program = 5,
    System = 6,
    Other = 7,
}

impl FileType {
    pub const ALL: [FileType; 8] = [
        FileType::Game,
        FileType::Media,
        FileType::Archive,
        FileType::VirtualDisk,
        FileType::Cache,
        FileType::Program,
        FileType::System,
        FileType::Other,
    ];

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Context {
    #[default]
    None,
    System,
    Game,
    Cache,
    Program,
}

fn lower(name: &OsStr) -> String {
    name.to_string_lossy().to_lowercase()
}

const SYSTEM_ROOT_DIRS: [&str; 8] = [
    "windows",
    "system volume information",
    "recovery",
    "$winreagent",
    "boot",
    "windows.old",
    "$windows.~bt",
    "$windows.~ws",
];
const CACHE_DIRS: [&str; 14] = [
    "cache",
    "caches",
    "cache2",
    "temp",
    "tmp",
    "dxcache",
    "glcache",
    "shadercache",
    "gpucache",
    "code cache",
    "inetcache",
    "crashdumps",
    "nv_cache",
    "d3dscache",
];
const GAME_DIRS: [&str; 5] = [
    "steamapps",
    "epic games",
    "xboxgames",
    "riot games",
    "gog games",
];

/// Context of a folder from its parent's context and its name; the first
/// matching rule wins.
pub fn child_context(parent: Context, dir_name: &OsStr, at_drive_root: bool) -> Context {
    if parent == Context::System {
        return Context::System;
    }
    let n = lower(dir_name);
    let n = n.as_str();
    if at_drive_root && SYSTEM_ROOT_DIRS.contains(&n) {
        return Context::System;
    }
    if CACHE_DIRS.contains(&n) {
        return Context::Cache;
    }
    if GAME_DIRS.contains(&n) {
        return Context::Game;
    }
    if matches!(parent, Context::Game | Context::Cache | Context::Program) {
        return parent;
    }
    if (at_drive_root && matches!(n, "program files" | "program files (x86)")) || n == "windowsapps"
    {
        return Context::Program;
    }
    Context::None
}

/// Type of a file from the context of its folder, then its extension.
pub fn file_type(ctx: Context, file_name: &OsStr) -> FileType {
    match ctx {
        Context::System => return FileType::System,
        Context::Cache => return FileType::Cache,
        Context::Game => return FileType::Game,
        Context::None | Context::Program => {}
    }
    let name = lower(file_name);
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "sys" => FileType::System,
        "pak" | "vpk" | "bsa" | "ba2" | "ucas" | "utoc" | "forge" | "wad" => FileType::Game,
        "mp4" | "mkv" | "mov" | "avi" | "wmv" | "webm" | "m4v" | "flv" | "ts" | "jpg" | "jpeg"
        | "png" | "gif" | "heic" | "webp" | "raw" | "cr2" | "nef" | "arw" | "dng" | "psd"
        | "tif" | "tiff" | "bmp" | "mp3" | "flac" | "wav" | "aac" | "ogg" | "m4a" | "opus" => {
            FileType::Media
        }
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "iso" | "msi" | "msix"
        | "msixbundle" | "appx" | "cab" => FileType::Archive,
        "vhd" | "vhdx" | "vmdk" | "vdi" | "qcow2" | "avhdx" | "img" => FileType::VirtualDisk,
        "tmp" | "temp" | "cache" | "dmp" => FileType::Cache,
        "exe" | "dll" | "jar" | "pyd" | "node" | "so" => FileType::Program,
        _ if ctx == Context::Program => FileType::Program,
        _ => FileType::Other,
    }
}

/// Context of the scan root, deduced from the safety rules.
pub fn root_context(root: &Path, rules: &RuleContext) -> Context {
    match classify(root, rules) {
        Verdict::Block(
            Block::WindowsDir
            | Block::SystemFiles
            | Block::OldWindows
            | Block::PageFile
            | Block::Hiberfil,
        ) => Context::System,
        Verdict::Block(Block::ProgramFiles | Block::WindowsApps) => Context::Program,
        Verdict::Warn(Warning::GameLauncher) => Context::Game,
        _ => Context::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_rules() {
        use Context::*;
        let c = |p, n: &str, r| child_context(p, OsStr::new(n), r);
        assert_eq!(c(None, "Windows", true), System);
        assert_eq!(c(None, "Windows", false), None);
        assert_eq!(c(System, "x", false), System);
        assert_eq!(c(None, "steamapps", false), Game);
        assert_eq!(c(Game, "shadercache", false), Cache);
        assert_eq!(c(Program, "steamapps", false), Game);
        assert_eq!(c(None, "Program Files", true), Program);
        assert_eq!(c(Program, "x", false), Program);
        assert_eq!(c(None, "Temp", false), Cache);
        assert_eq!(c(Cache, "x", false), Cache);
        assert_eq!(c(None, "WindowsApps", false), Program);
        assert_eq!(c(None, "Videos", false), None);
    }

    #[test]
    fn file_rules() {
        use Context as C;
        use FileType::*;
        let f = |c, n: &str| file_type(c, OsStr::new(n));
        assert_eq!(f(C::None, "movie.MKV"), Media);
        assert_eq!(f(C::None, "a.zip"), Archive);
        assert_eq!(f(C::None, "setup.msi"), Archive);
        assert_eq!(f(C::None, "disk.vhdx"), VirtualDisk);
        assert_eq!(f(C::None, "pagefile.sys"), System);
        assert_eq!(f(C::None, "app.exe"), Program);
        assert_eq!(f(C::None, "data.pak"), Game);
        assert_eq!(f(C::Game, "intro.mp4"), Game);
        assert_eq!(f(C::Cache, "x.png"), Cache);
        assert_eq!(f(C::System, "a.zip"), System);
        assert_eq!(f(C::Program, "readme.txt"), Program);
        assert_eq!(f(C::None, "notes.txt"), Other);
        assert_eq!(f(C::None, "MEMORY.DMP"), Cache);
        assert_eq!(f(C::None, "noext"), Other);
    }

    #[test]
    fn root_contexts() {
        let ctx = RuleContext {
            windows_dir: r"C:\Windows".into(),
            program_data: r"C:\ProgramData".into(),
            users_dir: r"C:\Users".into(),
            cloud_roots: vec![
                r"C:\Users\lenny\OneDrive".into(),
                r"C:\Users\lenny\Dropbox".into(),
            ],
        };
        let r = |p: &str| root_context(Path::new(p), &ctx);
        assert_eq!(r(r"C:\Windows"), Context::System);
        assert_eq!(r(r"C:\Program Files\X"), Context::Program);
        assert_eq!(r(r"D:\SteamLibrary\steamapps\common\G"), Context::Game);
        assert_eq!(r(r"D:\Data"), Context::None);
    }
}

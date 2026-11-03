//! Garde-fous : decide si un chemin peut etre supprime (Allow), avec
//! avertissement (Warn) ou jamais (Block).

use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    DriveRoot,
    WindowsDir,
    ProgramFiles,
    WindowsApps,
    ProgramData,
    PageFile,
    Hiberfil,
    SystemFiles,
    OldWindows,
    UsersRoot,
    ProfileRoot,
    AppDataRoot,
    WindowsUserData,
    RecycleBinContent,
}

impl Block {
    /// Vrai pour les blocages qui relevent du systeme (pas du profil utilisateur).
    pub fn is_system(self) -> bool {
        matches!(
            self,
            Block::WindowsDir
                | Block::ProgramFiles
                | Block::WindowsApps
                | Block::ProgramData
                | Block::PageFile
                | Block::Hiberfil
                | Block::SystemFiles
                | Block::OldWindows
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Warning {
    GameLauncher,
    CloudSync,
    AppData,
    VirtualDisk,
    OutlookData,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Warn(Warning),
    Block(Block),
}

#[derive(Clone, Debug)]
pub struct RuleContext {
    pub windows_dir: PathBuf,
    pub program_data: PathBuf,
    pub users_dir: PathBuf,
    pub cloud_roots: Vec<PathBuf>,
}

/// Composants en minuscules : le prefixe de lecteur (s'il existe) puis les
/// composants normaux. Le separateur de racine est ignore.
fn parts(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Prefix(p) => {
                let s = p.as_os_str().to_string_lossy().to_lowercase();
                Some(s.strip_prefix(r"\\?\").map(str::to_owned).unwrap_or(s))
            }
            Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect()
}

fn starts_with(path: &[String], prefix: &[String]) -> bool {
    !prefix.is_empty() && path.len() >= prefix.len() && path[..prefix.len()] == *prefix
}

pub fn classify(path: &Path, ctx: &RuleContext) -> Verdict {
    use Block as B;
    use Verdict::{Allow, Block as Blk, Warn};
    use Warning as W;

    let p = parts(path);

    // 1. Racine de disque.
    if p.len() <= 1 {
        return Blk(B::DriveRoot);
    }

    // 2. Premier composant sous la racine, sur n'importe quel disque.
    match p[1].as_str() {
        "$recycle.bin" => return Blk(B::RecycleBinContent),
        "program files" | "program files (x86)" => return Blk(B::ProgramFiles),
        "pagefile.sys" | "swapfile.sys" => return Blk(B::PageFile),
        "hiberfil.sys" => return Blk(B::Hiberfil),
        "system volume information" | "recovery" | "$winreagent" | "boot" | "bootmgr" => {
            return Blk(B::SystemFiles);
        }
        "windows.old" | "$windows.~bt" | "$windows.~ws" => return Blk(B::OldWindows),
        _ => {}
    }

    // 3. WindowsApps n'importe ou.
    if p.iter().any(|c| c == "windowsapps") {
        return Blk(B::WindowsApps);
    }

    // 4. Dossiers systeme du contexte.
    if starts_with(&p, &parts(&ctx.windows_dir)) {
        return Blk(B::WindowsDir);
    }
    if starts_with(&p, &parts(&ctx.program_data)) {
        return Blk(B::ProgramData);
    }

    // 5. Profils.
    let users = parts(&ctx.users_dir);
    let mut in_appdata = false;
    if starts_with(&p, &users) {
        let rest = &p[users.len()..];
        match rest.len() {
            0 => return Blk(B::UsersRoot),
            1 => return Blk(B::ProfileRoot),
            _ => {}
        }
        if rest[0] == "default" {
            return Blk(B::ProfileRoot);
        }
        let r: Vec<&str> = rest[1..].iter().map(String::as_str).collect();
        if r == ["appdata"] {
            return Blk(B::AppDataRoot);
        }
        if r.starts_with(&["appdata", "roaming", "microsoft"])
            || r.starts_with(&["appdata", "local", "microsoft"])
            || r.starts_with(&["appdata", "local", "packages"])
        {
            return Blk(B::WindowsUserData);
        }
        in_appdata = r.first() == Some(&"appdata");
    }

    // 6. Avertissements.
    let is_game = p
        .windows(2)
        .any(|w| w[0] == "steamapps" && w[1] == "common")
        || p.iter().any(|c| c == "epic games" || c == "xboxgames");
    if is_game {
        return Warn(W::GameLauncher);
    }
    if ctx
        .cloud_roots
        .iter()
        .any(|root| starts_with(&p, &parts(root)))
    {
        return Warn(W::CloudSync);
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "vhdx" | "vhd" => return Warn(W::VirtualDisk),
        "pst" | "ost" => return Warn(W::OutlookData),
        _ => {}
    }
    if in_appdata {
        return Warn(W::AppData);
    }
    Allow
}

impl RuleContext {
    /// Dossiers connus de Windows (API Shell) et racines de synchronisation cloud.
    pub fn from_system() -> RuleContext {
        let env_path = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };

        #[cfg(windows)]
        let (windows_dir, program_data, users_dir) = (
            known_folder::windows(),
            known_folder::program_data(),
            known_folder::user_profiles(),
        );
        #[cfg(not(windows))]
        let (windows_dir, program_data, users_dir): (
            Option<PathBuf>,
            Option<PathBuf>,
            Option<PathBuf>,
        ) = (None, None, None);

        let windows_dir = windows_dir
            .or_else(|| env_path("SystemRoot"))
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let program_data = program_data
            .or_else(|| env_path("ProgramData"))
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        let users_dir = users_dir
            .or_else(|| env_path("USERPROFILE").and_then(|p| p.parent().map(Path::to_path_buf)))
            .unwrap_or_else(|| PathBuf::from(r"C:\Users"));

        let mut cloud_roots: Vec<PathBuf> = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
            .iter()
            .filter_map(|k| env_path(k))
            .collect();
        if let Some(home) = env_path("USERPROFILE") {
            for name in ["Dropbox", "Google Drive", "My Drive", "iCloudDrive"] {
                cloud_roots.push(home.join(name));
            }
        }
        RuleContext {
            windows_dir,
            program_data,
            users_dir,
            cloud_roots,
        }
    }
}

#[cfg(windows)]
mod known_folder {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{
        FOLDERID_ProgramData, FOLDERID_UserProfiles, FOLDERID_Windows, KF_FLAG_DEFAULT,
        SHGetKnownFolderPath,
    };
    use windows::core::GUID;

    fn get(id: &GUID) -> Option<PathBuf> {
        // SAFETY: `id` est un GUID valide ; le PWSTR renvoye est libere avec
        // CoTaskMemFree apres copie.
        unsafe {
            let pwstr = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
            let s = OsString::from_wide(pwstr.as_wide());
            CoTaskMemFree(Some(pwstr.0 as *const _));
            Some(PathBuf::from(s))
        }
    }

    pub fn windows() -> Option<PathBuf> {
        get(&FOLDERID_Windows)
    }
    pub fn program_data() -> Option<PathBuf> {
        get(&FOLDERID_ProgramData)
    }
    pub fn user_profiles() -> Option<PathBuf> {
        get(&FOLDERID_UserProfiles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ctx() -> RuleContext {
        RuleContext {
            windows_dir: r"C:\Windows".into(),
            program_data: r"C:\ProgramData".into(),
            users_dir: r"C:\Users".into(),
            cloud_roots: vec![
                r"C:\Users\lenny\OneDrive".into(),
                r"C:\Users\lenny\Dropbox".into(),
            ],
        }
    }

    #[test]
    fn classify_table() {
        use super::Block as B;
        use Verdict::*;
        use Warning as W;
        let cases: &[(&str, Verdict)] = &[
            (r"C:\", Block(B::DriveRoot)),
            (r"D:\", Block(B::DriveRoot)),
            (r"C:\Windows\System32", Block(B::WindowsDir)),
            (r"c:\windows", Block(B::WindowsDir)),
            (r"D:\Program Files\Foo", Block(B::ProgramFiles)),
            (r"C:\Program Files (x86)", Block(B::ProgramFiles)),
            (
                r"C:\Program Files\Epic Games\Fortnite",
                Block(B::ProgramFiles),
            ),
            (r"E:\WindowsApps\Pkg", Block(B::WindowsApps)),
            (r"C:\ProgramData\X", Block(B::ProgramData)),
            (r"C:\pagefile.sys", Block(B::PageFile)),
            (r"C:\swapfile.sys", Block(B::PageFile)),
            (r"C:\hiberfil.sys", Block(B::Hiberfil)),
            (r"C:\System Volume Information", Block(B::SystemFiles)),
            (r"C:\Recovery\x", Block(B::SystemFiles)),
            (r"C:\$WinREAgent", Block(B::SystemFiles)),
            (r"C:\Boot", Block(B::SystemFiles)),
            (r"C:\bootmgr", Block(B::SystemFiles)),
            (r"C:\Windows.old\Users", Block(B::OldWindows)),
            (r"C:\$Windows.~BT", Block(B::OldWindows)),
            (r"C:\$Windows.~WS", Block(B::OldWindows)),
            (r"C:\Users", Block(B::UsersRoot)),
            (r"C:\Users\lenny", Block(B::ProfileRoot)),
            (r"C:\Users\Public", Block(B::ProfileRoot)),
            (r"C:\Users\Default\AppData\Local", Block(B::ProfileRoot)),
            (r"C:\Users\lenny\AppData", Block(B::AppDataRoot)),
            (
                r"C:\Users\lenny\AppData\Roaming\Microsoft\Protect",
                Block(B::WindowsUserData),
            ),
            (
                r"C:\Users\lenny\AppData\Local\Microsoft\Windows",
                Block(B::WindowsUserData),
            ),
            (
                r"C:\Users\lenny\AppData\Local\Packages\X",
                Block(B::WindowsUserData),
            ),
            (
                r"C:\$Recycle.Bin\S-1-5-21\$R123",
                Block(B::RecycleBinContent),
            ),
            (r"C:\Users\lenny\AppData\Local\Temp", Warn(W::AppData)),
            (
                r"C:\Users\lenny\AppData\Local\Docker\wsl\ext4.vhdx",
                Warn(W::VirtualDisk),
            ),
            (
                r"D:\SteamLibrary\steamapps\common\Game",
                Warn(W::GameLauncher),
            ),
            (r"D:\SteamLibrary\steamapps", Allow),
            (r"D:\Epic Games\Fortnite", Warn(W::GameLauncher)),
            (r"D:\XboxGames\Halo", Warn(W::GameLauncher)),
            (r"C:\Users\lenny\OneDrive", Warn(W::CloudSync)),
            (r"C:\Users\lenny\OneDrive\Docs", Warn(W::CloudSync)),
            (r"C:\Users\lenny\Documents\mail.pst", Warn(W::OutlookData)),
            (r"C:\Users\lenny\Documents\x.OST", Warn(W::OutlookData)),
            (r"C:\Users\lenny\Videos", Allow),
            (r"D:\Data\movie.mkv", Allow),
        ];
        for (p, v) in cases {
            assert_eq!(classify(Path::new(p), &ctx()), *v, "{p}");
        }
    }

    #[test]
    fn system_blocks() {
        assert!(Block::WindowsDir.is_system());
        assert!(!Block::ProfileRoot.is_system());
    }

    #[cfg(windows)]
    #[test]
    fn from_system_resolves_known_folders() {
        let c = RuleContext::from_system();
        assert!(c.windows_dir.join("System32").is_dir());
        assert!(c.users_dir.is_dir());
    }
}

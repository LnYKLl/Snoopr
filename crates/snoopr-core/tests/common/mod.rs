#![allow(dead_code)]

use std::fs::File;
use std::path::Path;

/// Crée une jonction `link` pointant vers `target` (sans droits administrateur).
pub fn make_junction(link: &Path, target: &Path) {
    let status = std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("lancement de cmd");
    assert!(status.success(), "mklink /J a échoué");
}

/// Ouvre le fichier en exclusivité (aucun partage) pour simuler un fichier verrouillé.
pub fn lock_file(path: &Path) -> File {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
        .expect("verrouillage du fichier")
}

/// Écrit un fichier de `len` octets en créant les dossiers parents.
pub fn write_file(path: &Path, len: usize) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, vec![7u8; len]).unwrap();
}

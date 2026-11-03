mod common;

use common::{lock_file, make_junction, write_file};
use snoopr_core::delete::permanent::delete_permanently;
use snoopr_core::delete::{Failure, FailureReason, ItemOutcome, reason_from_io};
use std::io;

#[test]
fn deletes_tree_including_readonly() {
    let t = tempfile::tempdir().unwrap();
    let d = t.path().join("d");
    write_file(&d.join("x.txt"), 10);
    let ro = d.join("sub").join("ro.txt");
    write_file(&ro, 10);
    let mut perm = std::fs::metadata(&ro).unwrap().permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(&ro, perm).unwrap();
    assert_eq!(delete_permanently(&d), ItemOutcome::Removed);
    assert!(!d.exists());
}

#[test]
fn locked_file_reports_in_use() {
    let t = tempfile::tempdir().unwrap();
    let d = t.path().join("d");
    write_file(&d.join("keep.txt"), 10);
    write_file(&d.join("gone.txt"), 10);
    let lock = lock_file(&d.join("keep.txt"));
    let out = delete_permanently(&d);
    assert_eq!(
        out,
        ItemOutcome::Partial(vec![Failure {
            path: d.join("keep.txt"),
            reason: FailureReason::InUse
        }])
    );
    assert!(!d.join("gone.txt").exists());
    assert!(d.join("keep.txt").exists());
    drop(lock);

    let single = t.path().join("single.txt");
    write_file(&single, 10);
    let lock = lock_file(&single);
    assert_eq!(
        delete_permanently(&single),
        ItemOutcome::Failed(Failure {
            path: single.clone(),
            reason: FailureReason::InUse
        })
    );
    drop(lock);
}

#[test]
fn missing_path_counts_as_removed() {
    let t = tempfile::tempdir().unwrap();
    assert_eq!(
        delete_permanently(&t.path().join("nope")),
        ItemOutcome::Removed
    );
}

#[test]
fn deletes_long_path_tree() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("long");
    let mut p = root.clone();
    for _ in 0..6 {
        p = p.join("segment_é😀_".repeat(5));
    }
    write_file(&p.join("é😀.txt"), 10);
    assert!(p.as_os_str().len() > 300);
    assert_eq!(delete_permanently(&root), ItemOutcome::Removed);
    assert!(!root.exists());
}

#[test]
fn does_not_follow_junction() {
    let t = tempfile::tempdir().unwrap();
    let outside = t.path().join("outside");
    write_file(&outside.join("precious.txt"), 10);
    let d = t.path().join("d");
    std::fs::create_dir_all(&d).unwrap();
    make_junction(&d.join("j"), &outside);
    assert_eq!(delete_permanently(&d), ItemOutcome::Removed);
    assert!(!d.exists());
    assert!(outside.join("precious.txt").exists());
}

#[test]
fn io_reason_mapping() {
    assert_eq!(
        reason_from_io(&io::Error::from_raw_os_error(32)),
        FailureReason::InUse
    );
    assert_eq!(
        reason_from_io(&io::Error::from_raw_os_error(5)),
        FailureReason::AccessDenied
    );
}

#[test]
fn deletes_tree_with_readonly_directory() {
    let t = tempfile::tempdir().unwrap();
    let d = t.path().join("d");
    let rod = d.join("rodir");
    write_file(&rod.join("f.txt"), 10);
    let mut perm = std::fs::metadata(&rod).unwrap().permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(&rod, perm).unwrap();
    assert_eq!(delete_permanently(&d), ItemOutcome::Removed);
    assert!(!d.exists());
}

#[test]
fn root_junction_removes_link_only() {
    let t = tempfile::tempdir().unwrap();
    let outside = t.path().join("outside");
    write_file(&outside.join("precious.txt"), 10);
    let j = t.path().join("j");
    make_junction(&j, &outside);
    assert_eq!(delete_permanently(&j), ItemOutcome::Removed);
    assert!(!j.exists());
    assert!(outside.join("precious.txt").exists());
}

#[test]
fn deletes_lone_surrogate_names() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    let weird = OsString::from_wide(&[0xD800, 0x61]);

    // Imbriqué dans la cible.
    let t = tempfile::tempdir().unwrap();
    let d = t.path().join("d");
    write_file(&d.join(&weird).join("f.txt"), 10);
    assert_eq!(delete_permanently(&d), ItemOutcome::Removed);
    assert!(!d.exists());

    // Cible racine.
    let root = t.path().join(&weird);
    write_file(&root.join("f.txt"), 10);
    assert_eq!(delete_permanently(&root), ItemOutcome::Removed);
    assert_eq!(
        std::fs::symlink_metadata(&root).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
}

#[test]
fn root_with_dot_component() {
    let t = tempfile::tempdir().unwrap();
    let d = t.path().join("d");
    write_file(&d.join("f.txt"), 10);
    let target = t.path().join(".").join("d");
    assert_eq!(delete_permanently(&target), ItemOutcome::Removed);
    assert!(!d.exists());
}

#[test]
fn pinned_open_failure_reports_in_use() {
    use std::os::windows::fs::OpenOptionsExt;
    let t = tempfile::tempdir().unwrap();
    let d = t.path().join("d");
    let nested = d.join("nested");
    write_file(&nested.join("child.txt"), 10);
    write_file(&d.join("gone.txt"), 10);
    // DELETE sans FILE_SHARE_DELETE : le dossier ne peut plus être ouvert pour suppression.
    let holder = std::fs::OpenOptions::new()
        .access_mode(0x0001_0000)
        .share_mode(1 | 2)
        .custom_flags(0x0200_0000)
        .open(&nested)
        .unwrap();
    assert_eq!(
        delete_permanently(&d),
        ItemOutcome::Partial(vec![Failure {
            path: nested.clone(),
            reason: FailureReason::InUse
        }])
    );
    assert!(nested.join("child.txt").exists());
    assert!(!d.join("gone.txt").exists());
    drop(holder);
}

/// Pose une balise de point d analyse inconnue (non lien) sur un dossier vide.
/// Retourne `false` si le système refuse (privilèges), le test est alors ignoré.
fn set_custom_reparse_tag(dir: &std::path::Path) -> bool {
    use std::ffi::c_void;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;

    let Ok(h) = std::fs::OpenOptions::new()
        .access_mode(0x0002 | 0x0100) // FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES
        .share_mode(7)
        .custom_flags(0x0200_0000 | 0x0020_0000) // BACKUP_SEMANTICS | OPEN_REPARSE_POINT
        .open(dir)
    else {
        return false;
    };
    let mut buf = Vec::new();
    buf.extend_from_slice(&0x8000_0123u32.to_le_bytes()); // balise inconnue, pas un lien
    buf.extend_from_slice(&4u16.to_le_bytes()); // 4 octets de données
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&[1, 2, 3, 4]);
    // SAFETY: `buf` est un tampon valide pour la durée de l'appel, sa taille est passée telle quelle.
    unsafe {
        DeviceIoControl(
            HANDLE(h.as_raw_handle()),
            FSCTL_SET_REPARSE_POINT,
            Some(buf.as_ptr().cast::<c_void>()),
            buf.len() as u32,
            None,
            0,
            None,
            None,
        )
    }
    .is_ok()
}

#[test]
fn deletes_directory_with_non_link_reparse_tag() {
    let t = tempfile::tempdir().unwrap();
    let d = t.path().join("d");
    write_file(&d.join("x.txt"), 10);
    let tagged = d.join("tagged");
    std::fs::create_dir_all(&tagged).unwrap();
    if !set_custom_reparse_tag(&tagged) {
        return; // privilèges insuffisants : test ignoré
    }
    assert_eq!(delete_permanently(&d), ItemOutcome::Removed);
    assert!(!d.exists());
}

mod common;

use common::{make_junction, write_file};
use snoopr_core::scan::fs::{read_dir_entries, to_verbatim};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

#[test]
fn lists_files_dirs_with_allocated_size_and_times() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(&tmp.path().join("a.bin"), 100_000);
    std::fs::create_dir(tmp.path().join("sub")).unwrap();
    let es = read_dir_entries(tmp.path()).unwrap();
    assert_eq!(es.len(), 2);
    let a = es.iter().find(|e| e.name == "a.bin").unwrap();
    assert!(!a.is_dir && a.alloc_size >= 100_000 && a.alloc_size % 512 == 0);
    assert!((snoopr_core::age::now_unix() - a.modified).abs() < 120);
    assert!(es.iter().any(|e| e.name == "sub" && e.is_dir && !e.is_link));
}

#[test]
fn junction_is_reported_as_link() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("t");
    std::fs::create_dir(&target).unwrap();
    make_junction(&tmp.path().join("j"), &target);
    let es = read_dir_entries(tmp.path()).unwrap();
    assert!(es.iter().any(|e| e.name == "j" && e.is_dir && e.is_link));
}

#[test]
fn reads_long_and_unicode_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let mut deep = tmp.path().to_path_buf();
    for i in 0..12 {
        deep.push(format!("dossier_très_long_{i:02}_{}", "x".repeat(20)));
    }
    assert!(deep.as_os_str().len() > 300);
    write_file(&deep.join("é😀.txt"), 10);
    let es = read_dir_entries(&deep).unwrap();
    assert_eq!(es[0].name, OsString::from("é😀.txt"));
}

#[test]
fn missing_dir_is_not_found() {
    let e = read_dir_entries(Path::new(r"C:\definitely\missing\snoopr")).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::NotFound);
}

#[test]
fn verbatim_forms() {
    assert_eq!(to_verbatim(Path::new(r"C:\a")), PathBuf::from(r"\\?\C:\a"));
    assert_eq!(
        to_verbatim(Path::new(r"\\srv\share\x")),
        PathBuf::from(r"\\?\UNC\srv\share\x")
    );
    assert_eq!(
        to_verbatim(Path::new(r"\\?\C:\a")),
        PathBuf::from(r"\\?\C:\a")
    );
}

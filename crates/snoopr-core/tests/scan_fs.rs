mod common;

use common::{make_junction, write_file};
use snoopr_core::scan::fs::{read_dir_entries, to_verbatim};
use snoopr_core::scan::{ScanProgress, start_scan};
use snoopr_core::tree::{NodeId, Tree};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};
use std::time::Duration;

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
fn lone_surrogate_names_round_trip() {
    use std::os::windows::ffi::OsStringExt;
    let tmp = tempfile::tempdir().unwrap();
    let name = OsString::from_wide(&[0xD800, 'a' as u16]);
    let dir = tmp.path().join(&name);
    std::fs::create_dir(&dir).unwrap();
    write_file(&dir.join("f.txt"), 10);
    let top = read_dir_entries(tmp.path()).unwrap();
    let listed = &top.iter().find(|e| e.is_dir).unwrap().name;
    assert_eq!(listed, &name);
    let es = read_dir_entries(&tmp.path().join(listed)).unwrap();
    assert_eq!(es.len(), 1);
    assert_eq!(es[0].name, OsString::from("f.txt"));
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

fn scan_dir(path: &Path) -> (Tree, Arc<ScanProgress>) {
    let t = Arc::new(RwLock::new(Tree::new(path.to_path_buf(), 0)));
    let h = start_scan(t.clone(), NodeId(0), path.to_path_buf());
    let progress = h.progress.clone();
    h.wait();
    let tree = Arc::try_unwrap(t).ok().unwrap().into_inner().unwrap();
    (tree, progress)
}

fn build_nested(root: &Path) {
    write_file(&root.join("a.txt"), 10_000);
    write_file(&root.join("sub").join("b.txt"), 20_000);
    write_file(&root.join("sub").join("deeper").join("c.txt"), 30_000);
}

#[test]
fn scans_nested_tree_with_allocated_sizes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    build_nested(root);
    let expected: u64 = [
        root.to_path_buf(),
        root.join("sub"),
        root.join("sub").join("deeper"),
    ]
    .iter()
    .flat_map(|d| read_dir_entries(d).unwrap())
    .filter(|e| !e.is_dir)
    .map(|e| e.alloc_size)
    .sum();
    let (tree, _) = scan_dir(root);
    let r = tree.node(tree.root());
    assert_eq!(r.files, 3);
    assert_eq!(r.size, expected);
    let sub = tree.child_by_name(tree.root(), "sub".as_ref()).unwrap();
    let deeper = tree.child_by_name(sub, "deeper".as_ref()).unwrap();
    assert!(tree.child_by_name(deeper, "c.txt".as_ref()).is_some());
}

#[test]
fn junction_loop_is_not_followed() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    build_nested(root);
    make_junction(&root.join("sub").join("loop"), root);
    let (tree, _) = scan_dir(root);
    assert_eq!(tree.node(tree.root()).files, 3);
    let sub = tree.child_by_name(tree.root(), "sub".as_ref()).unwrap();
    assert!(tree.child_by_name(sub, "loop".as_ref()).is_none());
}

#[test]
fn missing_dir_finishes_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let (tree, _) = scan_dir(&tmp.path().join("nope"));
    assert_eq!(tree.node(tree.root()).files, 0);
}

struct AclGuard(PathBuf, String);
impl Drop for AclGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("icacls")
            .arg(&self.0)
            .arg("/remove:d")
            .arg(&self.1)
            .stdout(std::process::Stdio::null())
            .status();
    }
}

#[test]
fn denied_dir_is_counted() {
    let tmp = tempfile::tempdir().unwrap();
    let locked = tmp.path().join("locked");
    std::fs::create_dir(&locked).unwrap();
    let user = std::env::var("USERNAME").unwrap();
    let _guard = AclGuard(locked.clone(), user.clone());
    let st = std::process::Command::new("icacls")
        .arg(&locked)
        .arg("/deny")
        .arg(format!("{user}:(RX)"))
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(st.success());
    let (_tree, progress) = scan_dir(tmp.path());
    assert_eq!(progress.denied.load(Ordering::Relaxed), 1);
}

#[test]
fn cancel_stops_quickly() {
    let t = Arc::new(RwLock::new(Tree::new(r"C:\Windows".into(), 0)));
    let h = start_scan(t.clone(), NodeId(0), r"C:\Windows".into());
    h.cancel();
    assert!(h.wait() < Duration::from_secs(5));
}

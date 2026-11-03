mod common;

use common::make_junction;
use snoopr_core::volume::{canonical_target, list_fixed_volumes, without_verbatim};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[test]
fn canonical_target_is_the_plain_real_path() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(t.path().join("a").join("b")).unwrap();
    let real = std::fs::canonicalize(t.path().join("a")).unwrap();
    let want = without_verbatim(&real);
    assert!(want.is_absolute());
    assert!(!want.to_string_lossy().starts_with(r"\\?\"), "{want:?}");
    assert!(want.ends_with("a"));

    let messy = t.path().join(".").join("a").join("b").join("..");
    assert_eq!(canonical_target(&messy), Some(want.clone()));
    let slashes = PathBuf::from(messy.to_string_lossy().replace('\\', "/"));
    assert_eq!(canonical_target(&slashes), Some(want.clone()));

    // A junction is resolved to the folder it points to.
    make_junction(&t.path().join("j"), &t.path().join("a"));
    assert_eq!(canonical_target(&t.path().join("j")), Some(want));

    // Relative to the current directory.
    let cwd = std::env::current_dir().unwrap();
    assert_eq!(canonical_target(Path::new(".")), canonical_target(&cwd));

    assert_eq!(canonical_target(&t.path().join("missing")), None);
}

/// Removes the SUBST letter when dropped, even if the test fails.
struct Subst(char);

impl Drop for Subst {
    fn drop(&mut self) {
        let _ = Command::new("subst")
            .arg(format!("{}:", self.0))
            .arg("/D")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Maps a free drive letter to `dir` with `subst` (no administrator rights needed).
fn subst(dir: &Path) -> Subst {
    for letter in 'P'..='Z' {
        if Path::new(&format!(r"{letter}:\")).exists() {
            continue;
        }
        let ok = Command::new("subst")
            .arg(format!("{letter}:"))
            .arg(dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            return Subst(letter);
        }
    }
    panic!("no free drive letter for subst");
}

#[test]
fn subst_drives_are_not_listed_and_resolve_to_their_folder() {
    let t = tempfile::tempdir().unwrap();
    let mapped = subst(t.path());
    let root = PathBuf::from(format!(r"{}:\", mapped.0));
    assert!(root.is_dir(), "{root:?} is mapped");
    let vols = list_fixed_volumes();
    assert!(
        vols.iter().all(|v| v.root != root),
        "{root:?} listed: {vols:?}"
    );
    assert_eq!(canonical_target(&root), canonical_target(t.path()));
}

//! Suppression définitive « au mieux » : ne suit jamais un lien ou une jonction
//! et liste précisément ce qui n'a pas pu être supprimé.
//!
//! Sous Windows, le parcours est entièrement relatif aux handles : le dossier cible est ouvert
//! une fois, ses entrées sont listées par ce handle et chaque enfant est ouvert relativement
//! au handle du parent (`NtCreateFile` + `RootDirectory`). Aucun chemin n'est ré-résolu, donc un
//! dossier remplacé par une jonction pendant la suppression ne peut pas rediriger la suppression.

use super::{Failure, ItemOutcome, reason_from_io};
use std::io;
use std::path::Path;

pub fn delete_permanently(path: &Path) -> ItemOutcome {
    let mut failures = Vec::new();
    let is_dir = imp::run(path, &mut failures);
    if failures.is_empty() {
        ItemOutcome::Removed
    } else if is_dir {
        ItemOutcome::Partial(failures)
    } else {
        ItemOutcome::Failed(failures.remove(0))
    }
}

fn is_not_found(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotFound
}

/// Le chemin ne sert qu'à l'affichage du rapport ; il n'est jamais rouvert.
fn push_failure(failures: &mut Vec<Failure>, path: &Path, e: &io::Error) {
    failures.push(Failure {
        path: path.to_path_buf(),
        reason: reason_from_io(e),
    });
}

#[cfg(windows)]
mod imp {
    use super::{is_not_found, push_failure};
    use crate::delete::Failure;
    use crate::scan::fs::{RawEntry, enumerate_handle, is_link_tag};
    use std::ffi::c_void;
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use std::path::Path;
    use windows::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows::Wdk::Storage::FileSystem::{
        FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
        FILE_SYNCHRONOUS_IO_NONALERT, NTCREATEFILE_CREATE_OPTIONS, NtCreateFile,
    };
    use windows::Win32::Foundation::{
        HANDLE, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError, UNICODE_STRING,
    };
    use windows::Win32::Storage::FileSystem::{
        DELETE, FILE_ACCESS_RIGHTS, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_TAG_INFO, FILE_BASIC_INFO,
        FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
        FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX,
        FILE_DISPOSITION_INFO_EX_FLAGS, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_FLAGS_AND_ATTRIBUTES, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, FileAttributeTagInfo, FileBasicInfo,
        FileDispositionInfo, FileDispositionInfoEx, GetFileInformationByHandleEx, SYNCHRONIZE,
        SetFileInformationByHandle,
    };
    use windows::Win32::System::IO::IO_STATUS_BLOCK;
    use windows::core::PWSTR;

    const ERROR_INVALID_PARAMETER: i32 = 87;
    const ERROR_NOT_SUPPORTED: i32 = 50;
    const ERROR_ACCESS_DENIED: i32 = 5;

    fn handle_of(f: &File) -> HANDLE {
        HANDLE(f.as_raw_handle())
    }

    fn win32_code(e: &windows::core::Error) -> i32 {
        let hr = e.code().0 as u32;
        if hr >> 16 == 0x8007 {
            (hr & 0xFFFF) as i32
        } else {
            hr as i32
        }
    }

    fn to_io(e: &windows::core::Error) -> io::Error {
        io::Error::from_raw_os_error(win32_code(e))
    }

    /// Point d'entrée : retourne `true` si la cible est un vrai dossier (non lien).
    pub fn run(path: &Path, failures: &mut Vec<Failure>) -> bool {
        match open_root(path, true) {
            Ok(f) => process(f, path, failures),
            Err(e) if is_not_found(&e) => false,
            Err(e) => {
                push_failure(failures, path, &e);
                std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
            }
        }
    }

    /// Ouvre la cible elle-même (sans suivre de lien). Le partage sans DELETE l'épingle.
    fn open_root(path: &Path, with_delete: bool) -> io::Result<File> {
        let mut access = FILE_LIST_DIRECTORY.0
            | FILE_READ_ATTRIBUTES.0
            | FILE_WRITE_ATTRIBUTES.0
            | SYNCHRONIZE.0;
        if with_delete {
            access |= DELETE.0;
        }
        let open = |access: u32| {
            OpenOptions::new()
                .access_mode(access)
                .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE).0)
                .custom_flags((FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT).0)
                .open(path)
        };
        match open(access) {
            Err(e) if with_delete && e.raw_os_error() == Some(ERROR_ACCESS_DENIED) => {
                // Probablement lecture seule : retire l'attribut via un handle sans DELETE.
                let f = open_root(path, false).map_err(|_| e)?;
                clear_readonly(&f)?;
                drop(f);
                open(access)
            }
            r => r,
        }
    }

    /// Attributs et tag de point d'analyse, lus sur le handle (aucun accès par chemin).
    fn attribute_tag(f: &File) -> io::Result<(u32, u32)> {
        let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
        // SAFETY: `info` est une structure valide en écriture et la taille passée est la sienne.
        unsafe {
            GetFileInformationByHandleEx(
                handle_of(f),
                FileAttributeTagInfo,
                (&raw mut info).cast::<c_void>(),
                std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
            )
        }
        .map_err(|e| to_io(&e))?;
        Ok((info.FileAttributes, info.ReparseTag))
    }

    /// Traite un objet ouvert (jamais par chemin). Retourne `true` pour un vrai dossier.
    fn process(f: File, display: &Path, failures: &mut Vec<Failure>) -> bool {
        let (attrs, tag) = match attribute_tag(&f) {
            Ok(v) => v,
            Err(e) => {
                push_failure(failures, display, &e);
                return false;
            }
        };
        if is_link_tag(attrs, tag) {
            // Jonction ou lien symbolique : on supprime l'objet lien lui-même, sans le traverser.
            delete_and_report(f, display, failures);
            false
        } else if attrs & FILE_ATTRIBUTE_DIRECTORY.0 != 0 {
            walk(f, display, failures);
            true
        } else {
            delete_and_report(f, display, failures);
            false
        }
    }

    fn delete_and_report(f: File, display: &Path, failures: &mut Vec<Failure>) {
        let r = delete_by_handle(&f);
        drop(f);
        if let Err(e) = r
            && !is_not_found(&e)
        {
            push_failure(failures, display, &e);
        }
    }

    fn walk(dir: File, display: &Path, failures: &mut Vec<Failure>) {
        let entries = match enumerate_handle(handle_of(&dir)) {
            Ok(v) => v,
            Err(e) => {
                push_failure(failures, display, &e);
                return;
            }
        };
        let before = failures.len();
        for entry in &entries {
            let child_display = display.join(&entry.name);
            match open_child(&dir, entry) {
                Ok(child) => {
                    process(child, &child_display, failures);
                }
                Err(e) if is_not_found(&e) => {}
                Err(e) => push_failure(failures, &child_display, &e),
            }
        }
        drop(entries);
        // Un dossier resté non vide à cause d'un enfant n'est pas une cause racine (R4).
        if failures.len() == before {
            delete_and_report(dir, display, failures);
        }
    }

    fn open_child(parent: &File, entry: &RawEntry) -> io::Result<File> {
        let mut access = FILE_READ_ATTRIBUTES.0 | FILE_WRITE_ATTRIBUTES.0 | SYNCHRONIZE.0;
        if entry.is_dir {
            access |= FILE_LIST_DIRECTORY.0;
        }
        match nt_open(parent, entry, access | DELETE.0) {
            Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED) => {
                // Probablement lecture seule : retire l'attribut via un handle sans DELETE.
                match nt_open(parent, entry, access) {
                    Ok(f) => {
                        clear_readonly(&f).map_err(|_| e)?;
                        drop(f);
                        nt_open(parent, entry, access | DELETE.0)
                    }
                    Err(_) => Err(e),
                }
            }
            r => r,
        }
    }

    /// Ouvre `entry` relativement au handle `parent`, sans suivre de point d'analyse.
    fn nt_open(parent: &File, entry: &RawEntry, access: u32) -> io::Result<File> {
        let name: Vec<u16> = entry.name.encode_wide().collect();
        let bytes = u16::try_from(name.len() * 2)
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        let uname = UNICODE_STRING {
            Length: bytes,
            MaximumLength: bytes,
            Buffer: PWSTR(name.as_ptr().cast_mut()),
        };
        let attrs = OBJECT_ATTRIBUTES {
            Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: handle_of(parent),
            ObjectName: &uname,
            Attributes: OBJ_CASE_INSENSITIVE,
            ..Default::default()
        };
        let kind = if entry.is_dir {
            FILE_DIRECTORY_FILE
        } else {
            FILE_NON_DIRECTORY_FILE
        };
        let options = NTCREATEFILE_CREATE_OPTIONS(
            FILE_OPEN_REPARSE_POINT.0 | FILE_SYNCHRONOUS_IO_NONALERT.0 | kind.0,
        );
        let mut handle = HANDLE::default();
        let mut iosb = IO_STATUS_BLOCK::default();
        // SAFETY: tous les pointeurs (`handle`, `attrs`, `uname`, `name`, `iosb`) pointent vers des
        // locaux valides pendant l'appel ; `RootDirectory` est le handle ouvert de `parent`.
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                FILE_ACCESS_RIGHTS(access),
                &attrs,
                &mut iosb,
                None,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                FILE_OPEN,
                options,
                None,
                0,
            )
        };
        if status.0 < 0 {
            // SAFETY: conversion pure d'un NTSTATUS en code d'erreur Win32.
            let code = unsafe { RtlNtStatusToDosError(status) };
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        // SAFETY: NtCreateFile a réussi : `handle` est un handle valide dont nous devenons
        // propriétaires ; `File` le fermera.
        Ok(unsafe { File::from_raw_handle(handle.0) })
    }

    /// Retire l'attribut lecture seule via le handle (nécessite FILE_WRITE_ATTRIBUTES).
    fn clear_readonly(f: &File) -> io::Result<()> {
        let attrs = f.metadata()?.file_attributes();
        let mut new = attrs & !FILE_ATTRIBUTE_READONLY.0;
        if new == 0 {
            new = FILE_ATTRIBUTE_NORMAL.0;
        }
        // Les dates à 0 ne sont pas modifiées.
        let info = FILE_BASIC_INFO {
            CreationTime: 0,
            LastAccessTime: 0,
            LastWriteTime: 0,
            ChangeTime: 0,
            FileAttributes: new,
        };
        // SAFETY: `info` est un FILE_BASIC_INFO valide et la taille passée est la sienne.
        unsafe {
            SetFileInformationByHandle(
                handle_of(f),
                FileBasicInfo,
                (&raw const info).cast::<c_void>(),
                std::mem::size_of::<FILE_BASIC_INFO>() as u32,
            )
        }
        .map_err(|e| to_io(&e))
    }

    /// Supprime l'objet désigné par le handle (sémantique POSIX, lecture seule ignorée).
    fn delete_by_handle(f: &File) -> io::Result<()> {
        let info = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_INFO_EX_FLAGS(
                FILE_DISPOSITION_FLAG_DELETE.0
                    | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                    | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
            ),
        };
        // SAFETY: `info` est un FILE_DISPOSITION_INFO_EX valide et la taille passée est la sienne.
        let r = unsafe {
            SetFileInformationByHandle(
                handle_of(f),
                FileDispositionInfoEx,
                (&raw const info).cast::<c_void>(),
                std::mem::size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
            )
        };
        match r {
            Ok(()) => Ok(()),
            Err(e)
                if matches!(
                    win32_code(&e),
                    ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED
                ) =>
            {
                // Windows ancien ou système de fichiers sans FileDispositionInfoEx.
                clear_readonly(f)?;
                let info = FILE_DISPOSITION_INFO { DeleteFile: true };
                // SAFETY: `info` est un FILE_DISPOSITION_INFO valide, taille passée = la sienne.
                unsafe {
                    SetFileInformationByHandle(
                        handle_of(f),
                        FileDispositionInfo,
                        (&raw const info).cast::<c_void>(),
                        std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
                    )
                }
                .map_err(|e| to_io(&e))
            }
            Err(e) => Err(to_io(&e)),
        }
    }
}

/// Repli non-Windows (non livré) : parcours à base de `std`.
#[cfg(not(windows))]
mod imp {
    use super::{is_not_found, push_failure};
    use crate::delete::Failure;
    use std::fs;
    use std::path::Path;

    pub fn run(path: &Path, failures: &mut Vec<Failure>) -> bool {
        let meta = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                if !is_not_found(&e) {
                    push_failure(failures, path, &e);
                }
                return false;
            }
        };
        if meta.is_dir() {
            let before = failures.len();
            match fs::read_dir(path) {
                Ok(rd) => {
                    for entry in rd.flatten() {
                        run(&entry.path(), failures);
                    }
                }
                Err(e) => push_failure(failures, path, &e),
            }
            if failures.len() == before
                && let Err(e) = fs::remove_dir(path)
                && !is_not_found(&e)
            {
                push_failure(failures, path, &e);
            }
            true
        } else {
            if let Err(e) = fs::remove_file(path)
                && !is_not_found(&e)
            {
                push_failure(failures, path, &e);
            }
            false
        }
    }
}

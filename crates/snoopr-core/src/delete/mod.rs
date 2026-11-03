//! Types partagés de suppression.

use std::io;
use std::path::PathBuf;

pub mod permanent;
pub mod recycle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteMode {
    RecycleBin,
    Permanent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureReason {
    InUse,
    AccessDenied,
    Cancelled,
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub path: PathBuf,
    pub reason: FailureReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemOutcome {
    Removed,
    Partial(Vec<Failure>),
    Failed(Failure),
}

/// Convertit une erreur d'E/S en raison d'échec (32/33 : partage → `InUse`, 5 : `AccessDenied`).
pub fn reason_from_io(e: &io::Error) -> FailureReason {
    match e.raw_os_error() {
        Some(32) | Some(33) => FailureReason::InUse,
        Some(5) => FailureReason::AccessDenied,
        _ => FailureReason::Other(e.to_string()),
    }
}

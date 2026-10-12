//! The only file that talks to the operating system: finding the engine
//! binary, asking it whether it is up, and running its event stream. The
//! decisions about events are all in `logic.rs`.

// Removed when lib.rs uses everything in this file (slice C).
#![allow(dead_code)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;

use trigger_core::Availability;

use crate::logic::Engine;

/// The file name of the engine's command line tool.
fn binary_name(engine: Engine) -> String {
    if cfg!(windows) {
        format!("{}.exe", engine.as_str())
    } else {
        engine.as_str().to_string()
    }
}

/// Folders the engine's tool usually lives in, searched after `PATH`. A
/// program started from the Dock or the Start menu may have a short `PATH`
/// that misses them. A wrong entry costs one failed file check.
#[cfg(not(windows))]
fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = [
        "/usr/local/bin",
        "/opt/homebrew/bin",
        "/opt/podman/bin",
        "/usr/bin",
        "/Applications/Docker.app/Contents/Resources/bin",
        "/snap/bin",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for sub in [".orbstack/bin", ".rd/bin", ".docker/bin"] {
            dirs.push(home.join(sub));
        }
    }
    dirs
}

/// Same as above for Windows. The folders come from environment variables,
/// never from a hard coded drive letter.
#[cfg(windows)]
fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for (var, sub) in [
        ("ProgramFiles", r"Docker\Docker\resources\bin"),
        ("ProgramFiles", r"RedHat\Podman"),
        ("ProgramFiles", "Podman"),
        ("LOCALAPPDATA", r"Programs\Podman"),
        ("USERPROFILE", r".rd\bin"),
    ] {
        if let Some(base) = std::env::var_os(var) {
            dirs.push(PathBuf::from(base).join(sub));
        }
    }
    dirs
}

/// `PATH` first, then the extra folders, without empty entries or repeats.
fn search_dirs(path: Option<OsString>, extra: Vec<PathBuf>) -> Vec<PathBuf> {
    let from_path = path
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut dirs: Vec<PathBuf> = Vec::new();
    for dir in from_path.into_iter().chain(extra) {
        if !dir.as_os_str().is_empty() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// The first folder that holds a file with this name wins. A folder with that
/// name does not count.
fn find_in(dirs: &[PathBuf], file_name: &str) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| dir.join(file_name))
        .find(|candidate| candidate.is_file())
}

/// Where the engine's tool is, or `None`. Looked up once per run of the app
/// and kept, because the catalogue asks on the UI thread. No login shell is
/// started to widen `PATH`.
pub fn find_engine(engine: Engine) -> Option<PathBuf> {
    static DOCKER: OnceLock<Option<PathBuf>> = OnceLock::new();
    static PODMAN: OnceLock<Option<PathBuf>> = OnceLock::new();
    let slot = match engine {
        Engine::Docker => &DOCKER,
        Engine::Podman => &PODMAN,
    };
    slot.get_or_init(|| {
        let dirs = search_dirs(std::env::var_os("PATH"), candidate_dirs());
        find_in(&dirs, &binary_name(engine))
    })
    .clone()
}

/// Whether there is an engine to ask. It only checks that a tool was found;
/// whether the engine answers is checked by `start`.
pub fn availability() -> Availability {
    if find_engine(Engine::Docker).is_some() || find_engine(Engine::Podman).is_some() {
        Availability::Available
    } else {
        Availability::Unsupported {
            reason: "neither docker nor podman was found".into(),
        }
    }
}

#[cfg(test)]
mod tests;

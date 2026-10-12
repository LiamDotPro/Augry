//! The only file that talks to the operating system: finding the engine
//! binary, asking it whether it is up, and running its event stream. The
//! decisions about events are all in `logic.rs`.

// Removed when lib.rs uses everything in this file (slice C).
#![allow(dead_code)]

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

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

/// How long the engine gets to answer `version`.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// How often a running child is checked.
const POLL: Duration = Duration::from_millis(10);

/// A command that does not open a console window on Windows.
fn command(program: &Path) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new(program);
        // CREATE_NO_WINDOW
        command.creation_flags(0x0800_0000);
        command
    }
    #[cfg(not(windows))]
    {
        Command::new(program)
    }
}

/// Runs the command and returns what it printed to stdout, trimmed, when it
/// exited with success and printed something within `limit`. A command that
/// takes longer is killed and reaped. Only for commands with short output.
fn answer(mut command: Command, limit: Duration) -> Option<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < limit => std::thread::sleep(POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    let out = out.trim().to_string();
    (!out.is_empty()).then_some(out)
}

/// What asking an engine found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// No tool for this engine was found.
    Missing,
    /// The tool is there but the engine did not answer `version` in time.
    Silent(PathBuf),
    /// The engine answered.
    Ready(PathBuf),
}

/// Finds the engine's tool and runs `version` against it. Takes up to five
/// seconds, so never call it from the UI thread.
pub fn probe(engine: Engine) -> Probe {
    let Some(path) = find_engine(engine) else {
        return Probe::Missing;
    };
    let mut version = command(&path);
    version.args(["version", "--format", "{{.Server.Version}}"]);
    match answer(version, VERSION_TIMEOUT) {
        Some(_) => Probe::Ready(path),
        None => Probe::Silent(path),
    }
}

#[cfg(test)]
mod tests;

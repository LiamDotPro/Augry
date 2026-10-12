//! The git command line, std only. This is the pattern of the app's
//! `git/cli.rs` copied here, because a trigger crate may not depend on the
//! app (research note R02). Together with `os.rs` it is the crate's OS layer.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

use std::fmt;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The time most calls get before they are killed.
pub const TIMEOUT: Duration = Duration::from_secs(3);
/// `git fetch` talks to the network and gets longer.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(10);

/// Why git could not be run. A non zero exit is not an error: see `GitOutput`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitError {
    NotInstalled,
    /// The command took longer than its timeout and was killed. Holds the
    /// first argument, such as `fetch`.
    Timeout(String),
    Io(String),
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::NotInstalled => write!(f, "git was not found"),
            GitError::Timeout(what) => write!(f, "git {what} timed out"),
            GitError::Io(why) => write!(f, "git could not run: {why}"),
        }
    }
}

impl std::error::Error for GitError {}

/// What a finished git command printed. `ok` is false for a non zero exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// The programs tried in order. Bare `git` first, like the app. A macOS app
/// started from the Dock has a short PATH, so a missing `git` falls back to
/// the two usual Homebrew locations.
#[cfg(target_os = "macos")]
const PROGRAMS: &[&str] = &["git", "/opt/homebrew/bin/git", "/usr/local/bin/git"];
#[cfg(not(target_os = "macos"))]
const PROGRAMS: &[&str] = &["git"];

/// Runs `git <args>` in `cwd`. Err only when git cannot run or times out.
pub fn run(cwd: &Path, args: &[&str], timeout: Duration) -> Result<GitOutput, GitError> {
    run_with(PROGRAMS, cwd, args, timeout)
}

/// Like `run`, trying each program in turn while the spawn says NotFound.
pub(crate) fn run_with(
    programs: &[&str],
    cwd: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<GitOutput, GitError> {
    // A missing working directory also makes the spawn say NotFound, which
    // would look like a missing git, so rule it out first.
    if !cwd.is_dir() {
        return Err(GitError::Io(format!(
            "{} is not a directory",
            cwd.display()
        )));
    }
    for program in programs {
        match spawn(program, cwd, args) {
            Ok(child) => return wait(child, args.first().copied().unwrap_or(""), timeout),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(GitError::Io(e.to_string())),
        }
    }
    Err(GitError::NotInstalled)
}

fn spawn(program: &str, cwd: &Path, args: &[&str]) -> std::io::Result<Child> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn()
}

/// Reads a pipe on its own thread so a full pipe cannot stop git from exiting.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> Option<JoinHandle<Vec<u8>>> {
    let mut pipe = pipe?;
    std::thread::Builder::new()
        .name("trigger-git-pipe".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            bytes
        })
        .ok()
}

fn collect(reader: Option<JoinHandle<Vec<u8>>>) -> String {
    let bytes = reader.and_then(|h| h.join().ok()).unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn wait(mut child: Child, what: &str, timeout: Duration) -> Result<GitOutput, GitError> {
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitError::Timeout(what.to_string()));
            }
            Ok(None) => std::thread::sleep(POLL),
            Err(e) => return Err(GitError::Io(e.to_string())),
        }
    };
    Ok(GitOutput {
        ok: status.success(),
        stdout: collect(stdout),
        stderr: collect(stderr),
    })
}

/// Stdout of a successful command with trailing line breaks removed. None when
/// git failed, timed out, or printed nothing: callers treat all three as
/// "not available".
pub fn value(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = run(cwd, args, TIMEOUT).ok()?;
    if !out.ok {
        return None;
    }
    let text = out.stdout.trim_end_matches(['\r', '\n']);
    (!text.is_empty()).then(|| text.to_string())
}

mod repo;

// Removed when lib.rs uses GitRepo.
#[allow(unused_imports)]
pub use repo::GitRepo;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod testutil;

#[cfg(test)]
mod testutil_tests;

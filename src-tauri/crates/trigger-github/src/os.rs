//! The only file that talks to the operating system: finds `gh` and runs it.
//! What to run and how to read the answer is decided in `logic/cli.rs` and
//! `logic/response.rs`; this file only starts children.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use trigger_core::Availability;

use crate::logic::cli::{
    api_args, candidate_dirs, exe_name, login_from_body, signed_in, Gh, AUTH_ARGS,
};
use crate::logic::response::{parse_gh_output, GhResponse};

/// Why the source is unavailable when `gh` is not installed.
pub const NOT_FOUND: &str = "GitHub CLI not found. Install it from https://cli.github.com";

static GH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Where `gh` is: the first `gh` file in the folders of `candidate_dirs`. It
/// is looked up once and remembered, found or not, so it costs nothing after
/// the first call. Installing `gh` while the app runs is noticed on restart.
pub fn find_gh() -> Option<&'static Path> {
    GH.get_or_init(|| {
        let path_var = std::env::var_os("PATH");
        let home =
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
        let dirs = candidate_dirs(path_var.as_deref(), home.as_deref(), cfg!(windows));
        first_file(&dirs, exe_name(cfg!(windows)))
    })
    .as_deref()
}

/// The first of `dirs` that holds a file called `name`.
fn first_file(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter().map(|dir| dir.join(name)).find(|p| p.is_file())
}

/// Whether `gh` was found. It starts no child process, because the catalogue
/// command calls it on the UI thread. Whether `gh` is signed in is checked
/// when the source starts.
pub fn availability() -> Availability {
    match find_gh() {
        Some(_) => Availability::Available,
        None => Availability::Unsupported {
            reason: NOT_FOUND.into(),
        },
    }
}

/// What a child that finished on its own left behind. Its stderr is thrown
/// away: `gh` prints the login and token hints there, and nothing in this
/// crate may log or keep them.
#[derive(Debug)]
pub struct Finished {
    pub stdout: Vec<u8>,
    /// None when the process was ended by a signal.
    pub exit: Option<i32>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    /// The program could not be started. Carries the OS error text.
    Spawn(String),
    /// It was still running when the time ran out, and was killed.
    TimedOut,
}

/// Runs `exe` with `args` and waits at most `timeout`. The child gets no
/// stdin, so `gh` can never stop to ask a question, and the two environment
/// variables that keep it from prompting or printing update notices. On
/// Windows it gets no console window. Stdout is read on its own thread so a
/// big answer cannot fill the pipe and stall the child.
pub fn run(exe: &Path, args: &[String], timeout: Duration) -> Result<Finished, RunError> {
    let mut command = Command::new(exe);
    command
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    no_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|e| RunError::Spawn(e.to_string()))?;
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(RunError::Spawn("no stdout pipe".into()));
    };
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::Builder::new()
        .name("trigger-github-read".into())
        .spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
    if let Err(e) = reader {
        let _ = child.kill();
        let _ = child.wait();
        return Err(RunError::Spawn(e.to_string()));
    }

    let deadline = Instant::now() + timeout;
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::TimedOut);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Spawn(e.to_string()));
            }
        }
    };
    // The reader ends when the pipe closes, which is when the child exits. The
    // wait is only a guard against a grandchild that kept the pipe open.
    let stdout = rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default();
    Ok(Finished { stdout, exit })
}

/// The time in whole seconds since the Unix epoch.
pub fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// How long one request may take before it counts as no answer.
const GET_TIMEOUT: Duration = Duration::from_secs(30);
/// `gh auth status` makes a network call to check the token, so it gets less
/// time than a request but more than a local command.
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// The real `gh`: one child process per request, one at a time because the
/// poll loop is serial.
pub struct GhCli {
    exe: PathBuf,
}

impl GhCli {
    pub fn at(exe: &Path) -> GhCli {
        GhCli {
            exe: exe.to_path_buf(),
        }
    }
}

impl Gh for GhCli {
    fn get(&self, path: &str, etag: Option<&str>) -> GhResponse {
        match run(&self.exe, &api_args(path, etag), GET_TIMEOUT) {
            Ok(done) => parse_gh_output(&done.stdout, done.exit),
            Err(RunError::TimedOut) => {
                GhResponse::Transport("gh did not answer within 30 seconds".into())
            }
            Err(RunError::Spawn(why)) => GhResponse::Transport(format!("could not run gh: {why}")),
        }
    }

    fn login(&self) -> Option<String> {
        match self.get("user", None) {
            GhResponse::Ok { body, .. } => login_from_body(&body),
            _ => None,
        }
    }
}

/// Whether `gh` is signed in to github.com, from `gh auth status --json
/// hosts`. Ok(false) means no login. Err is anything that stops the question
/// being answered (could not start, too slow, exit code other than 0), which
/// is not the same thing and must not be shown as "not signed in". The output
/// is read but never logged or kept.
pub fn auth_ok(exe: &Path) -> Result<bool, String> {
    let args: Vec<String> = AUTH_ARGS.iter().map(|s| s.to_string()).collect();
    let done = run(exe, &args, AUTH_TIMEOUT).map_err(|e| match e {
        RunError::TimedOut => "gh auth status did not answer within 10 seconds".to_string(),
        RunError::Spawn(why) => format!("could not run gh: {why}"),
    })?;
    match done.exit {
        Some(0) => Ok(signed_in(&String::from_utf8_lossy(&done.stdout))),
        Some(code) => Err(format!("gh auth status exited with code {code}")),
        None => Err("gh auth status was stopped before it answered".to_string()),
    }
}

#[cfg(windows)]
fn no_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn no_window(_command: &mut Command) {}

/// Tests that start children take this lock, so a test that writes a script
/// file never has it open while another test forks (which makes Linux refuse
/// to run the script with "text file busy").
#[cfg(all(test, unix))]
pub(crate) fn spawn_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod cli_tests;
#[cfg(test)]
mod find_tests;
#[cfg(test)]
mod run_tests;

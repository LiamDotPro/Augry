//! The only file that talks to the operating system: finds `gh` and runs it.
//! What to run and how to read the answer is decided in `logic/cli.rs` and
//! `logic/response.rs`; this file only starts children.

// Removed in the last card of slice C, when the poll loop uses everything here.
#![allow(dead_code)]

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use trigger_core::Availability;

pub fn availability() -> Availability {
    Availability::Unsupported {
        reason: "not implemented yet".into(),
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
mod run_tests;

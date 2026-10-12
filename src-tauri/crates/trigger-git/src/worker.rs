//! The thread that joins the OS layer to the pure logic. It owns every
//! repository slot, waits for file events and for the polled kinds' times,
//! and sends events to the sink. Glue only: the decisions are in `logic`.

// Removed when lib.rs reads everything in this file.
#![allow(dead_code)]

mod folder;
mod repo;

// The allow is removed when lib.rs uses these.
#[allow(unused_imports)]
pub use folder::FolderSlot;
#[allow(unused_imports)]
pub use repo::{Entry, RepoSlot};

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use trigger_core::Sink;

/// What the thread is told.
pub enum Msg {
    /// A watched file changed in the repository slot with this index.
    Changed(usize),
    /// The handle was dropped.
    Stop,
}

/// The longest the thread sleeps without looking at its channel, so a stop
/// is noticed within this time even if its message were lost.
const IDLE: Duration = Duration::from_millis(500);

/// The loop of the `trigger-git` thread. Sleeps until the earliest deadline
/// or the next message, then lets every slot do what is due and sends the
/// events. Returns when told to stop, when the sender side is gone, or when
/// the sink is closed. `keep` holds the file watchers; they live as long as
/// the loop and stop when it returns.
pub fn run(
    mut repos: Vec<RepoSlot>,
    mut folders: Vec<FolderSlot>,
    keep: Vec<Box<dyn Send>>,
    rx: Receiver<Msg>,
    sink: Sink,
) {
    let _keep = keep;
    loop {
        let now = Instant::now();
        let wake = repos
            .iter()
            .filter_map(|r| r.deadline())
            .chain(folders.iter().map(|f| f.deadline()))
            .min();
        let wait = wake
            .map(|at| at.saturating_duration_since(now))
            .unwrap_or(IDLE)
            .min(IDLE);
        match rx.recv_timeout(wait) {
            Ok(Msg::Changed(index)) => {
                if let Some(repo) = repos.get_mut(index) {
                    repo.touch(Instant::now());
                }
            }
            Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {}
        }

        let now = Instant::now();
        let from_repos = repos.iter_mut().flat_map(|r| r.tick(now));
        let events: Vec<_> = from_repos
            .chain(folders.iter_mut().flat_map(|f| f.tick(now)))
            .collect();
        for event in events {
            if sink.send(event).is_err() {
                return;
            }
        }
    }
}

/// Starts the `trigger-git` thread.
pub fn spawn(
    repos: Vec<RepoSlot>,
    folders: Vec<FolderSlot>,
    keep: Vec<Box<dyn Send>>,
    rx: Receiver<Msg>,
    sink: Sink,
) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("trigger-git".into())
        .spawn(move || run(repos, folders, keep, rx, sink))
        .map(|_| ())
}

#[cfg(test)]
mod tests;

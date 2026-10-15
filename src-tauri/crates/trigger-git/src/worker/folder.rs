//! One folder watched for `git.repo-cloned`. It has no file watcher: the
//! folder is listed on the rule's interval and `CloneScan` says which
//! children are new. The listing is passed in as a function, so tests do not
//! need a real folder.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use trigger_core::Event;

use super::repo::event;
use crate::logic::polled::{cloned_payload, CloneScan};

pub struct FolderSlot {
    id: String,
    folder: PathBuf,
    interval: Duration,
    next_poll: Instant,
    scan: CloneScan,
    /// Names of the children of a folder that hold a repository.
    list: fn(&Path) -> BTreeSet<String>,
}

impl FolderSlot {
    /// The first `tick` is due at once and becomes the baseline.
    pub fn new(
        id: String,
        folder: PathBuf,
        interval_s: u64,
        now: Instant,
        list: fn(&Path) -> BTreeSet<String>,
    ) -> FolderSlot {
        FolderSlot {
            id,
            folder,
            interval: Duration::from_secs(interval_s),
            next_poll: now,
            scan: CloneScan::new(),
            list,
        }
    }

    /// When `tick` next has something to do.
    pub fn deadline(&self) -> Instant {
        self.next_poll
    }

    /// Lists the folder if it is due and returns one event per new repository.
    pub fn tick(&mut self, now: Instant) -> Vec<Event> {
        if now < self.next_poll {
            return Vec::new();
        }
        self.next_poll = now + self.interval;
        let names = (self.list)(&self.folder);
        self.scan
            .observe(names)
            .iter()
            .map(|name| {
                event(
                    &self.id,
                    "git.repo-cloned",
                    cloned_payload(&self.folder, name),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;

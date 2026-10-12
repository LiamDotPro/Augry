//! Pure transition logic for the three polled kinds. The glue polls a `Repo`
//! (or scans a folder) on the rule's interval and feeds each answer to one of
//! these; they say whether the answer fires. No git, no clock, no threads.
//! Decision 10 of plans/003-trigger-git/epic.md.

use trigger_core::Value;

use super::Payload;

/// `git.behind-remote`: fires when the count of commits behind the upstream
/// goes from 0 to above 0.
#[derive(Debug, Default)]
pub struct BehindWatch {
    last: Option<u32>,
}

impl BehindWatch {
    pub fn new() -> BehindWatch {
        BehindWatch::default()
    }

    /// Feeds one poll. `behind` is what `Repo::behind` returned. Returns the
    /// count when this poll fires.
    ///
    /// - The first count is a baseline and never fires, even above 0.
    /// - After a fire the count must return to 0 before it can fire again, so
    ///   2 then 5 is one event, and 0, 2, 0, 1 is two.
    /// - None (no upstream, git failed) says nothing: the last count stays.
    pub fn observe(&mut self, behind: Option<u32>) -> Option<u32> {
        let now = behind?;
        let fires = self.last == Some(0) && now > 0;
        self.last = Some(now);
        fires.then_some(now)
    }
}

/// The payload of `git.behind-remote`: exactly the fields listed in `KINDS`.
pub fn behind_payload(repo: &str, branch: Option<&str>, behind: u32) -> Payload {
    let mut out = Payload::new();
    out.insert("repo".into(), Value::from(repo));
    out.insert("branch".into(), Value::from(branch.unwrap_or_default()));
    out.insert("behind".into(), Value::from(behind));
    out
}

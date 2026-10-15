//! The questions the glue asks about a repository, behind a trait so the
//! glue can be tested with a fake and without git. `GitRepo` in `git.rs` is
//! the real one (Decision 12 of plans/003-trigger-git/epic.md).

use super::RepoSnapshot;

pub trait Repo: Send {
    /// What the repository looks like now. Err holds a short reason, such as
    /// "not a git repository".
    fn snapshot(&self) -> Result<RepoSnapshot, String>;

    /// Commits the current branch is behind its upstream, counted from the
    /// remote refs on disk, after a `git fetch` first when `fetch` is true.
    /// None when there is no upstream (or HEAD is detached, or git failed).
    fn behind(&self, fetch: bool) -> Option<u32>;

    /// Age in whole seconds of the oldest modified tracked file in the work
    /// tree. None when nothing tracked is modified (or git failed).
    fn oldest_change_secs(&self) -> Option<u64>;
}

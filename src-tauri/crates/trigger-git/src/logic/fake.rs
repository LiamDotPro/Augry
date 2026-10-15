//! A `Repo` for tests. Clones share their state, so a test can keep one
//! handle to change what the repository says while the code under test owns
//! another.

use std::sync::{Arc, Mutex};

use super::repo::Repo;
use super::RepoSnapshot;

#[derive(Default)]
struct State {
    snapshot: Option<Result<RepoSnapshot, String>>,
    behind: Option<u32>,
    age: Option<u64>,
    snapshot_calls: u32,
    fetch_calls: u32,
    plain_behind_calls: u32,
}

#[derive(Clone, Default)]
pub struct FakeRepo {
    state: Arc<Mutex<State>>,
}

impl FakeRepo {
    /// A repository whose snapshot is `snapshot`, with no upstream and no
    /// modified files.
    pub fn new(snapshot: RepoSnapshot) -> FakeRepo {
        let repo = FakeRepo::default();
        repo.set_snapshot(Ok(snapshot));
        repo
    }

    pub fn set_snapshot(&self, snapshot: Result<RepoSnapshot, String>) {
        self.state.lock().unwrap().snapshot = Some(snapshot);
    }

    pub fn set_behind(&self, behind: Option<u32>) {
        self.state.lock().unwrap().behind = behind;
    }

    pub fn set_age(&self, age: Option<u64>) {
        self.state.lock().unwrap().age = age;
    }

    pub fn snapshot_calls(&self) -> u32 {
        self.state.lock().unwrap().snapshot_calls
    }

    /// How many times `behind(true)` was called.
    pub fn fetch_calls(&self) -> u32 {
        self.state.lock().unwrap().fetch_calls
    }

    /// How many times `behind(false)` was called.
    pub fn plain_behind_calls(&self) -> u32 {
        self.state.lock().unwrap().plain_behind_calls
    }
}

impl Repo for FakeRepo {
    fn snapshot(&self) -> Result<RepoSnapshot, String> {
        let mut state = self.state.lock().unwrap();
        state.snapshot_calls += 1;
        state
            .snapshot
            .clone()
            .unwrap_or_else(|| Err("no snapshot set".into()))
    }

    fn behind(&self, fetch: bool) -> Option<u32> {
        let mut state = self.state.lock().unwrap();
        if fetch {
            state.fetch_calls += 1;
        } else {
            state.plain_behind_calls += 1;
        }
        state.behind
    }

    fn oldest_change_secs(&self) -> Option<u64> {
        self.state.lock().unwrap().age
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}

    #[test]
    fn a_boxed_repo_can_move_to_a_thread() {
        assert_send::<Box<dyn Repo>>();
        assert_send::<FakeRepo>();
    }

    #[test]
    fn answers_what_it_was_told_and_counts_calls() {
        let snap = RepoSnapshot {
            branch: Some("main".into()),
            ..RepoSnapshot::default()
        };
        let fake = FakeRepo::new(snap.clone());
        let repo: Box<dyn Repo> = Box::new(fake.clone());

        assert_eq!(repo.snapshot(), Ok(snap));
        assert_eq!(repo.behind(false), None);
        assert_eq!(repo.oldest_change_secs(), None);

        fake.set_behind(Some(2));
        fake.set_age(Some(90));
        assert_eq!(repo.behind(true), Some(2));
        assert_eq!(repo.oldest_change_secs(), Some(90));
        assert_eq!(
            (
                fake.snapshot_calls(),
                fake.fetch_calls(),
                fake.plain_behind_calls()
            ),
            (1, 1, 1)
        );
    }

    #[test]
    fn a_snapshot_can_be_made_to_fail() {
        let fake = FakeRepo::default();
        assert_eq!(fake.snapshot(), Err("no snapshot set".to_string()));
        fake.set_snapshot(Err("not a git repository".into()));
        assert_eq!(fake.snapshot(), Err("not a git repository".to_string()));
    }
}

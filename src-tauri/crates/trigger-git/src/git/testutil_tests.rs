use super::testutil::{git_available, scratch_path, TempRepo};

#[test]
fn a_temp_repo_starts_on_main_and_can_commit() {
    if !git_available() {
        return;
    }
    let repo = TempRepo::new();
    assert_eq!(repo.git(&["symbolic-ref", "--short", "HEAD"]), "main");
    repo.commit("a.txt", "first");
    assert_eq!(repo.git(&["rev-parse", "HEAD"]).len(), 40);
    assert_eq!(repo.git(&["log", "-1", "--format=%s"]), "first");
}

#[test]
fn try_git_reports_failure_without_panicking() {
    if !git_available() {
        return;
    }
    let repo = TempRepo::new();
    assert!(!repo.try_git(&["no-such-subcommand"]).ok);
}

#[test]
fn a_clone_tracks_origin_and_both_are_removed_on_drop() {
    if !git_available() {
        return;
    }
    let origin = TempRepo::new();
    origin.commit("a.txt", "first");
    let clone = origin.clone_repo();
    assert_eq!(
        clone.git(&["rev-parse", "--abbrev-ref", "@{u}"]),
        "origin/main"
    );
    let (origin_path, clone_path) = (origin.path.clone(), clone.path.clone());
    drop(clone);
    drop(origin);
    assert!(!origin_path.exists());
    assert!(!clone_path.exists());
}

#[test]
fn scratch_paths_are_unique_and_not_created() {
    let (a, b) = (scratch_path("x"), scratch_path("x"));
    assert_ne!(a, b);
    assert!(!a.exists());
}

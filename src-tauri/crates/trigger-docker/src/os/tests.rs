use super::*;

/// A fresh folder under the system temp folder. The tests remove it.
fn temp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "trigger-docker-{label}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_binary_name_has_the_platform_extension() {
    let expected = if cfg!(windows) {
        "docker.exe"
    } else {
        "docker"
    };
    assert_eq!(binary_name(Engine::Docker), expected);
    let expected = if cfg!(windows) {
        "podman.exe"
    } else {
        "podman"
    };
    assert_eq!(binary_name(Engine::Podman), expected);
}

#[test]
fn path_comes_first_then_the_extra_folders() {
    let path = std::env::join_paths(["/a", "/b"]).unwrap();
    let dirs = search_dirs(Some(path), vec![PathBuf::from("/c"), PathBuf::from("/d")]);
    let expected: Vec<PathBuf> = ["/a", "/b", "/c", "/d"].iter().map(PathBuf::from).collect();
    assert_eq!(dirs, expected);
}

#[test]
fn repeats_and_empty_entries_are_dropped() {
    let path = std::env::join_paths(["/a", "", "/a"]).unwrap();
    let dirs = search_dirs(Some(path), vec![PathBuf::from("/a"), PathBuf::from("/c")]);
    let expected: Vec<PathBuf> = ["/a", "/c"].iter().map(PathBuf::from).collect();
    assert_eq!(dirs, expected);
}

#[test]
fn a_missing_path_still_gives_the_extra_folders() {
    let dirs = search_dirs(None, vec![PathBuf::from("/c")]);
    assert_eq!(dirs, vec![PathBuf::from("/c")]);
}

#[test]
fn find_in_returns_the_first_folder_with_the_file() {
    let first = temp_dir("first");
    let second = temp_dir("second");
    std::fs::write(second.join("docker"), b"").unwrap();
    let dirs = vec![first.clone(), second.clone()];
    assert_eq!(find_in(&dirs, "docker"), Some(second.join("docker")));
    std::fs::write(first.join("docker"), b"").unwrap();
    assert_eq!(find_in(&dirs, "docker"), Some(first.join("docker")));
    assert_eq!(find_in(&dirs, "podman"), None);
    std::fs::remove_dir_all(first).unwrap();
    std::fs::remove_dir_all(second).unwrap();
}

#[test]
fn a_folder_named_like_the_tool_does_not_count() {
    let dir = temp_dir("folder");
    std::fs::create_dir(dir.join("docker")).unwrap();
    assert_eq!(find_in(std::slice::from_ref(&dir), "docker"), None);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_candidate_folders_are_not_empty_and_have_no_blanks() {
    let dirs = candidate_dirs();
    assert!(!dirs.is_empty());
    assert!(dirs.iter().all(|d| !d.as_os_str().is_empty()));
}

#[cfg(not(windows))]
#[test]
fn the_unix_candidates_include_the_usual_places() {
    let dirs = candidate_dirs();
    assert!(dirs.contains(&PathBuf::from("/usr/local/bin")));
    assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
}

#[test]
fn find_engine_gives_the_same_answer_twice() {
    assert_eq!(find_engine(Engine::Docker), find_engine(Engine::Docker));
}

#[test]
fn availability_is_one_of_the_two_answers_it_can_give() {
    match availability() {
        Availability::Available => {}
        Availability::Unsupported { reason } => assert!(reason.contains("docker")),
        other => panic!("unexpected {other:?}"),
    }
}

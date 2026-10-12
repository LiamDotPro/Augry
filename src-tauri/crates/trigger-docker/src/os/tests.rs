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

/// A command that runs a shell script. The script ends with `exec` when it
/// should stay alive, so killing the shell also ends the sleep.
#[cfg(unix)]
fn sh(script: &str) -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", script]);
    command
}

#[cfg(unix)]
#[test]
fn answer_returns_trimmed_stdout_on_success() {
    let got = answer(sh("echo '  29.4.3  '"), Duration::from_secs(5));
    assert_eq!(got, Some("29.4.3".to_string()));
}

#[cfg(unix)]
#[test]
fn answer_is_none_when_the_command_fails() {
    assert_eq!(answer(sh("exit 1"), Duration::from_secs(5)), None);
    assert_eq!(answer(sh("echo hi; exit 1"), Duration::from_secs(5)), None);
}

#[cfg(unix)]
#[test]
fn answer_is_none_when_the_command_prints_nothing() {
    assert_eq!(answer(sh("exit 0"), Duration::from_secs(5)), None);
    assert_eq!(answer(sh("echo '   '"), Duration::from_secs(5)), None);
}

#[cfg(unix)]
#[test]
fn answer_kills_a_command_that_is_too_slow() {
    let started = Instant::now();
    let got = answer(sh("exec sleep 30"), Duration::from_millis(200));
    assert_eq!(got, None);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn answer_is_none_when_the_program_does_not_exist() {
    let missing = Command::new("/nonexistent/trigger-docker-test-program");
    assert_eq!(answer(missing, Duration::from_secs(5)), None);
}

#[test]
fn probe_is_consistent_with_find_engine() {
    for engine in [Engine::Docker, Engine::Podman] {
        match (find_engine(engine), probe(engine)) {
            (None, Probe::Missing) => {}
            (Some(found), Probe::Silent(path) | Probe::Ready(path)) => assert_eq!(found, path),
            (found, probe) => panic!("{found:?} does not fit {probe:?}"),
        }
    }
}

#[test]
fn the_docker_command_line_is_the_one_in_the_epic() {
    assert_eq!(
        events_args(Engine::Docker),
        [
            "events",
            "--format",
            "{{json .}}",
            "--filter",
            "type=container",
            "--filter",
            "type=image"
        ]
    );
}

#[test]
fn the_podman_command_line_asks_for_json() {
    assert_eq!(
        events_args(Engine::Podman),
        [
            "events",
            "--format",
            "json",
            "--filter",
            "type=container",
            "--filter",
            "type=image"
        ]
    );
}

#[test]
fn no_command_line_uses_since() {
    for engine in [Engine::Docker, Engine::Podman] {
        assert!(!events_args(engine).contains(&"--since"));
    }
}

#[cfg(unix)]
#[test]
fn child_lines_reads_lines_then_none_when_the_child_exits() {
    let mut lines = ChildLines::spawn(sh("echo one; printf 'two\\r\\n'; printf three")).unwrap();
    assert_eq!(lines.next_line(), Some("one".to_string()));
    assert_eq!(lines.next_line(), Some("two".to_string()));
    assert_eq!(lines.next_line(), Some("three".to_string()));
    assert_eq!(lines.next_line(), None);
    // Reaps the child. Killing one that has exited is fine, twice too.
    let killer = lines.killer();
    killer.kill();
    killer.kill();
}

#[cfg(unix)]
#[test]
fn killing_ends_a_child_that_never_prints_again() {
    let mut lines = ChildLines::spawn(sh("echo up; exec sleep 30")).unwrap();
    assert_eq!(lines.next_line(), Some("up".to_string()));
    let killer = lines.killer();
    let started = Instant::now();
    let reader = std::thread::spawn(move || lines.next_line());
    std::thread::sleep(Duration::from_millis(100));
    killer.kill();
    assert_eq!(reader.join().unwrap(), None);
    assert!(started.elapsed() < Duration::from_secs(5));
    killer.kill();
}

#[cfg(unix)]
#[test]
fn killing_waits_so_the_child_is_gone() {
    let lines = ChildLines::spawn(sh("exec sleep 30")).unwrap();
    let child = lines.child.clone();
    lines.killer().kill();
    // `try_wait` on a child that was already waited for still reports it.
    let status = child.lock().unwrap().try_wait().unwrap();
    assert!(status.is_some());
}

#[test]
fn a_program_that_does_not_exist_fails_to_spawn() {
    let missing = Command::new("/nonexistent/trigger-docker-test-program");
    assert!(ChildLines::spawn(missing).is_err());
    let real = RealEngine::new(
        Engine::Docker,
        PathBuf::from("/nonexistent/trigger-docker-test-program"),
    );
    assert!(real.spawn().is_err());
    assert_eq!(real.engine(), Engine::Docker);
}

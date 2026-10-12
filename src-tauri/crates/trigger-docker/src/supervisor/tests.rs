use super::*;

#[test]
fn backoff_doubles_from_one_second() {
    let seconds: Vec<u64> = (0..6).map(|n| backoff(n).as_secs()).collect();
    assert_eq!(seconds, vec![1, 2, 4, 8, 16, 32]);
}

#[test]
fn backoff_stops_at_sixty_seconds() {
    assert_eq!(backoff(6), Duration::from_secs(60));
    assert_eq!(backoff(7), Duration::from_secs(60));
    assert_eq!(backoff(1000), Duration::from_secs(60));
    assert_eq!(backoff(u32::MAX), Duration::from_secs(60));
}

#[test]
fn stop_wakes_a_long_wait_at_once() {
    let stop = Arc::new(Stop::default());
    assert!(!stop.is_stopped());
    let waiter = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let stopped = stop.wait(Duration::from_secs(60));
            (stopped, started.elapsed())
        })
    };
    std::thread::sleep(Duration::from_millis(50));
    stop.stop();
    let (stopped, elapsed) = waiter.join().unwrap();
    assert!(stopped);
    assert!(elapsed < Duration::from_secs(5));
    assert!(stop.is_stopped());
}

#[test]
fn wait_times_out_when_nobody_stops() {
    let stop = Stop::default();
    assert!(!stop.wait(Duration::from_millis(10)));
}

#[test]
fn wait_returns_at_once_when_already_stopped() {
    let stop = Stop::default();
    stop.stop();
    assert!(stop.wait(Duration::from_secs(60)));
}

mod loop_tests {
    use super::super::fakes::*;
    use super::*;
    use serde_json::json;

    fn started_rule() -> Armed {
        armed("docker.container-started", json!({}))
    }

    #[test]
    fn lines_are_parsed_matched_and_emitted_in_order() {
        let process = FakeProcess::new(
            Engine::Docker,
            vec![Script::Lines(vec![START, "not json at all", DIE])],
        );
        let (seen, emit) = recorder();
        let (_delays, pause) = recording_pause();
        let rules = vec![started_rule(), armed("docker.container-stopped", json!({}))];
        let sup = Supervisor::start(process, rules, emit, pause).unwrap();
        wait_until("two events", || seen.lock().unwrap().len() == 2);
        sup.stop();
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].1, "docker.container-started");
        assert_eq!(seen[0].2["name"], json!("r01c"));
        assert_eq!(seen[1].1, "docker.container-stopped");
        assert_eq!(seen[1].2["name"], json!("r01x"));
    }

    #[test]
    fn the_running_engine_replaces_the_one_the_parser_guessed() {
        // The line is Docker shaped, but the process says it is Podman.
        let process = FakeProcess::new(Engine::Podman, vec![Script::Lines(vec![START])]);
        let (seen, emit) = recorder();
        let (_delays, pause) = recording_pause();
        let rules = vec![
            armed("docker.container-started", json!({ "engine": "podman" })),
            armed("docker.container-started", json!({ "engine": "docker" })),
        ];
        let sup = Supervisor::start(process, rules, emit, pause).unwrap();
        wait_until("one event", || !seen.lock().unwrap().is_empty());
        sup.stop();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].2["engine"], json!("podman"));
    }

    #[test]
    fn a_line_no_rule_wants_emits_nothing() {
        let process = FakeProcess::new(Engine::Docker, vec![Script::Lines(vec![DIE])]);
        let (seen, emit) = recorder();
        let (delays, pause) = recording_pause();
        let sup = Supervisor::start(process, vec![started_rule()], emit, pause).unwrap();
        wait_until("a restart", || !delays.lock().unwrap().is_empty());
        sup.stop();
        assert!(seen.lock().unwrap().is_empty());
    }
}

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

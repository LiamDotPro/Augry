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

---
id: T07
epic: 007-trigger-clock
title: Fire clock.daily-first-use from step
status: todo
depends_on: [T06]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/logic.rs
  - src-tauri/crates/trigger-clock/src/logic/tests.rs
---

## Goal

`step` fires `clock.daily-first-use` once per local date, after the clock has run normally for `cfg.normal_for`. A machine that wakes from a night asleep fires 60 seconds after waking, not on the jump step.

## Read first

- `plans/007-trigger-clock/epic.md` Decision 5 (daily-first-use) and Decision 7
- `plans/007-trigger-clock/research/R03-sleep-and-wake.md` Implication 4 (`last_daily`, `normal_since`)
- `src-tauri/crates/trigger-clock/src/logic.rs` the `step` from T06

## Steps

1. In `src-tauri/crates/trigger-clock/src/logic.rs`, delete everything from the line that starts `/// The rules due at` down to, but not including, the line `/// How long the thread may wait before the next step: until the earliest`. That removes the old `step`. Paste this text in its place, keeping one blank line before and after. It adds `fire_daily` and one line in `step` that calls it. `fire_daily` does nothing, and does not record the date, when no `clock.daily-first-use` rule is armed.

```rust
/// `clock.daily-first-use`: fires once per local date, after the clock has run
/// normally for `cfg.normal_for`. A machine that slept all night therefore
/// fires one `normal_for` after waking, not on the jump step itself.
fn fire_daily<Tz: TimeZone>(
    state: &mut State,
    tz: &Tz,
    now: SystemTime,
    rules: &[Armed],
    out: &mut Vec<Fire>,
) {
    let today = in_zone(tz, now).date_naive();
    let calm = now.duration_since(state.normal_since).unwrap_or_default() >= state.cfg.normal_for;
    if state.last_daily == Some(today) || !calm {
        return;
    }
    let mut daily = rules
        .iter()
        .filter(|a| a.kind == "clock.daily-first-use")
        .peekable();
    if daily.peek().is_none() {
        return;
    }
    state.last_daily = Some(today);
    for armed in daily {
        out.push(Fire {
            rule: armed.id.clone(),
            kind: armed.kind.clone(),
            payload: BTreeMap::from([("date".to_string(), Value::from(today.to_string()))]),
        });
    }
}

/// The rules due at `now`. `tz` is the zone cron expressions and dates are
/// read in: `&chrono::Local` in the app, a fixed zone in tests.
pub fn step<Tz: TimeZone>(
    state: &mut State,
    tz: &Tz,
    now: SystemTime,
    rules: &[Armed],
) -> Vec<Fire> {
    let mut out = Vec::new();
    let Ok(gap) = now.duration_since(state.prev_wall) else {
        // The wall clock went backwards (NTP, or the user): fire nothing and
        // plan again from the new time.
        state.prev_wall = now;
        state.normal_since = now;
        state.replan();
        return out;
    };
    let jumped = gap > state.cfg.jump;
    if jumped {
        state.normal_since = now;
    }
    fire_due(state, tz, now, jumped, rules, &mut out);
    fire_daily(state, tz, now, rules, &mut out);
    state.prev_wall = now;
    out
}
```

2. Append these tests to the end of `src-tauri/crates/trigger-clock/src/logic/tests.rs`, after a blank line. Change nothing above them.

```rust

#[test]
fn daily_first_use_fires_once_after_the_clock_has_run_normally_for_a_minute() {
    let start = at(2026, 3, 6, 10, 0, 0);
    let rules = [armed("d", "clock.daily-first-use", json!({}))];
    let mut state = State::new(ClockConfig::default(), start);
    assert!(run(&mut state, start + secs(30), &rules).is_empty());
    let fired = run(&mut state, start + secs(60), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].kind, "clock.daily-first-use");
    assert_eq!(fired[0].payload["date"], json!("2026-03-06"));
    assert!(walk(&mut state, start + secs(60), start + secs(3600), &rules).is_empty());
}

#[test]
fn daily_first_use_fires_the_next_day_after_midnight() {
    let start = at(2026, 3, 6, 23, 58, 0);
    let rules = [armed("d", "clock.daily-first-use", json!({}))];
    let mut state = State::new(ClockConfig::default(), start);
    let fired = walk(&mut state, start, at(2026, 3, 7, 0, 5, 0), &rules);
    assert_eq!(fired.len(), 2);
    assert_eq!(fired[0].payload["date"], json!("2026-03-06"));
    assert_eq!(fired[1].payload["date"], json!("2026-03-07"));
}

#[test]
fn daily_first_use_waits_a_minute_after_waking_from_a_night_asleep() {
    let evening = at(2026, 3, 6, 23, 0, 0);
    let rules = [armed("d", "clock.daily-first-use", json!({}))];
    let mut state = State::new(ClockConfig::default(), evening);
    walk(&mut state, evening, evening + secs(120), &rules);
    // Wake at 07:00 the next morning: the jump step does not fire.
    let wake = at(2026, 3, 7, 7, 0, 0);
    assert!(run(&mut state, wake, &rules).is_empty());
    assert!(run(&mut state, wake + secs(30), &rules).is_empty());
    let fired = run(&mut state, wake + secs(60), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].payload["date"], json!("2026-03-07"));
    assert!(run(&mut state, wake + secs(90), &rules).is_empty());
}

#[test]
fn a_nap_during_the_day_does_not_fire_daily_first_use_again() {
    let start = at(2026, 3, 6, 10, 0, 0);
    let rules = [armed("d", "clock.daily-first-use", json!({}))];
    let mut state = State::new(ClockConfig::default(), start);
    assert_eq!(walk(&mut state, start, start + secs(120), &rules).len(), 1);
    let woke = start + secs(120 + 3 * 3600);
    assert!(run(&mut state, woke, &rules).is_empty());
    assert!(walk(&mut state, woke, woke + secs(300), &rules).is_empty());
}

#[test]
fn daily_first_use_does_nothing_without_a_daily_rule() {
    let start = at(2026, 3, 6, 10, 0, 0);
    let mut state = State::new(ClockConfig::default(), start);
    assert!(walk(&mut state, start, start + secs(300), &[]).is_empty());
    assert_eq!(state.last_daily, None);
}

#[test]
fn a_short_config_runs_in_milliseconds() {
    let cfg = ClockConfig {
        tick: Duration::from_millis(20),
        jump: Duration::from_millis(60),
        normal_for: Duration::from_millis(40),
    };
    let start = at(2026, 3, 6, 10, 0, 0);
    let rules = [armed("d", "clock.daily-first-use", json!({}))];
    let mut state = State::new(cfg, start);
    assert!(run(&mut state, start + Duration::from_millis(20), &rules).is_empty());
    assert_eq!(
        run(&mut state, start + Duration::from_millis(40), &rules).len(),
        1
    );
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock` passes: 31 tests in the library (6 new, in `logic::tests`), 1 in `readme`, 1 ignored in `live`
- [ ] `cd src-tauri && cargo test -p trigger-clock logic::tests::daily_first_use_waits_a_minute_after_waking_from_a_night_asleep` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0. Warnings about template text (`still has template code`) are expected until the glue card in slice C replaces `start`; they do not fail the check.
- [ ] `git status --porcelain` lists only paths under `files:`
- [ ] `wc -l src-tauri/crates/trigger-clock/src/logic.rs` prints a number under 400

## Out of scope

- Persisting `last_daily` across restarts (a documented limit: the README says so in T09).
- Reading any idle or lock signal.
- Event kinds: slice B.
- Editing `lib.rs`.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->

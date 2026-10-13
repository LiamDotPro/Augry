---
id: T06
epic: 007-trigger-clock
title: Handle sleep jumps, catch up and backwards clocks in step
status: done
depends_on: [T05]
size: M
model: haiku
files:
  - src-tauri/crates/trigger-clock/src/logic.rs
  - src-tauri/crates/trigger-clock/src/logic/tests.rs
---

## Goal

`step` detects a wall clock gap above `cfg.jump`. A rule with `catch_up` then fires once for the latest missed time with `caught_up: true`; a rule without it skips the missed times. A wall clock that goes backwards fires nothing and plans every rule again.

## Read first

- `plans/007-trigger-clock/epic.md` Decision 7 (JUMP, catch up, backwards jump)
- `plans/007-trigger-clock/research/R03-sleep-and-wake.md` Implication 3 (the three cases of `step`)
- `src-tauri/crates/trigger-clock/src/logic.rs` the `fire_due` and `step` from T05 that you replace

## Steps

1. In `src-tauri/crates/trigger-clock/src/logic.rs`, delete everything from the line `/// Schedule and interval rules.` down to, but not including, the line `/// How long the thread may wait before the next step: until the earliest`. That removes the old `fire_due` and the old `step`. Paste this text in their place, keeping one blank line before and after. `fire_due` gains a `jumped` argument; `step` computes it from the gap and handles a backwards clock first.

```rust
/// Schedule and interval rules. `jumped` is true when the machine slept.
fn fire_due<Tz: TimeZone>(
    state: &mut State,
    tz: &Tz,
    now: SystemTime,
    jumped: bool,
    rules: &[Armed],
    out: &mut Vec<Fire>,
) {
    for armed in rules {
        if !matches!(armed.kind.as_str(), "clock.schedule" | "clock.interval") {
            continue;
        }
        let cursor = *state
            .cursors
            .entry(armed.id.clone())
            .or_insert_with(|| next_due(armed, tz, state.prev_wall));
        let Some(cursor) = cursor else { continue };
        let (slots, next) = due_slots(armed, tz, cursor, now);
        state.cursors.insert(armed.id.clone(), next);
        let fires: Vec<(SystemTime, bool)> = if !jumped {
            slots.into_iter().map(|t| (t, false)).collect()
        } else if armed.parsed.catch_up() {
            slots.last().map(|t| (*t, true)).into_iter().collect()
        } else {
            Vec::new()
        };
        for (due, caught_up) in fires {
            out.push(Fire {
                rule: armed.id.clone(),
                kind: armed.kind.clone(),
                payload: due_payload(due, caught_up),
            });
        }
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
    state.prev_wall = now;
    out
}
```

2. Append these tests to the end of `src-tauri/crates/trigger-clock/src/logic/tests.rs`, after a blank line. Change nothing above them.

```rust

#[test]
fn a_gap_of_exactly_jump_is_still_normal() {
    let start = at(2026, 3, 6, 12, 0, 0);
    let rules = [armed("i", "clock.interval", json!({ "every_s": 30 }))];
    let mut state = State::new(ClockConfig::default(), start);
    let fired = run(&mut state, start + secs(90), &rules);
    assert_eq!(fired.len(), 3);
}

#[test]
fn after_a_six_hour_jump_catch_up_fires_once_for_the_latest_missed_time() {
    let start = at(2026, 3, 6, 22, 0, 0);
    let rules = [armed(
        "a",
        "clock.schedule",
        json!({ "cron": "0 * * * *", "catch_up": true }),
    )];
    let mut state = State::new(ClockConfig::default(), start);
    walk(&mut state, start, start + secs(120), &rules);
    // Six hours pass in one step. Hourly slots 23:00 to 04:00 were missed.
    let fired = run(&mut state, start + secs(120 + 6 * 3600), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(due_of(&fired[0]), millis(at(2026, 3, 7, 4, 0, 0)));
    assert_eq!(fired[0].payload["caught_up"], json!(true));
    // Back to normal: the next slot fires on time and is not a catch up.
    let woke = start + secs(120 + 6 * 3600);
    let fired = walk(&mut state, woke, at(2026, 3, 7, 5, 1, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(due_of(&fired[0]), millis(at(2026, 3, 7, 5, 0, 0)));
    assert_eq!(fired[0].payload["caught_up"], json!(false));
}

#[test]
fn after_a_jump_without_catch_up_missed_slots_are_skipped() {
    let start = at(2026, 3, 6, 22, 0, 0);
    let rules = [armed("a", "clock.schedule", json!({ "cron": "0 * * * *" }))];
    let mut state = State::new(ClockConfig::default(), start);
    walk(&mut state, start, start + secs(120), &rules);
    let woke = start + secs(120 + 6 * 3600);
    assert!(run(&mut state, woke, &rules).is_empty());
    // The cursor moved past the missed slots: 05:00 is next, nothing is owed.
    assert!(run(&mut state, woke + secs(30), &rules).is_empty());
    let fired = walk(&mut state, woke, at(2026, 3, 7, 5, 1, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(due_of(&fired[0]), millis(at(2026, 3, 7, 5, 0, 0)));
}

#[test]
fn a_jump_with_nothing_missed_fires_nothing_even_with_catch_up() {
    let start = at(2026, 3, 6, 8, 0, 0);
    let rules = [armed(
        "a",
        "clock.schedule",
        json!({ "cron": "0 9 * * *", "catch_up": true }),
    )];
    let mut state = State::new(ClockConfig::default(), start);
    walk(&mut state, start, start + secs(60), &rules);
    // A 10 minute nap that ends before 09:00.
    assert!(run(&mut state, start + secs(60 + 600), &rules).is_empty());
}

#[test]
fn an_interval_with_catch_up_fires_once_after_a_jump() {
    let start = at(2026, 3, 6, 12, 0, 0);
    let rules = [armed(
        "i",
        "clock.interval",
        json!({ "every_s": 3600, "catch_up": true }),
    )];
    let mut state = State::new(ClockConfig::default(), start);
    walk(&mut state, start, start + secs(60), &rules);
    // 5.5 hours later: slots at +1 h to +5 h were missed.
    let fired = run(&mut state, start + secs(60 + 19_800), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(due_of(&fired[0]), millis(start + secs(5 * 3600)));
    assert_eq!(fired[0].payload["caught_up"], json!(true));
    // The next slot keeps the original rhythm: +6 h.
    let woke = start + secs(60 + 19_800);
    let fired = walk(&mut state, woke, start + secs(6 * 3600 + 5), &rules);
    assert_eq!(due_of(&fired[0]), millis(start + secs(6 * 3600)));
}

#[test]
fn an_interval_without_catch_up_skips_the_missed_slots() {
    let start = at(2026, 3, 6, 12, 0, 0);
    let rules = [armed("i", "clock.interval", json!({ "every_s": 3600 }))];
    let mut state = State::new(ClockConfig::default(), start);
    walk(&mut state, start, start + secs(60), &rules);
    assert!(run(&mut state, start + secs(60 + 19_800), &rules).is_empty());
}

#[test]
fn a_backwards_jump_fires_nothing_and_plans_again() {
    let start = at(2026, 3, 6, 9, 0, 0);
    let rules = [armed(
        "a",
        "clock.schedule",
        json!({ "cron": "30 9 * * *" }),
    )];
    let mut state = State::new(ClockConfig::default(), start);
    walk(&mut state, start, start + secs(600), &rules);
    // The clock is set back an hour, to 08:10.
    let back = at(2026, 3, 6, 8, 10, 0);
    assert!(run(&mut state, back, &rules).is_empty());
    assert_eq!(state.prev_wall, back);
    assert_eq!(state.normal_since, back);
    // 09:30 comes round again and the rule fires for it.
    let fired = walk(&mut state, back, at(2026, 3, 6, 9, 31, 0), &rules);
    assert_eq!(fired.len(), 1);
    assert_eq!(due_of(&fired[0]), millis(at(2026, 3, 6, 9, 30, 0)));
}
```

3. Run `cd src-tauri && cargo fmt -p trigger-clock`, then the acceptance commands.

## Acceptance

- [ ] `cd src-tauri && cargo test -p trigger-clock` passes: 25 tests in the library (7 new, in `logic::tests`), 1 in `readme`, 1 ignored in `live`
- [ ] `cd src-tauri && cargo test -p trigger-clock logic::tests::after_a_six_hour_jump_catch_up_fires_once_for_the_latest_missed_time` passes
- [ ] `cd src-tauri && cargo test -p trigger-clock logic::tests::a_backwards_jump_fires_nothing_and_plans_again` passes
- [ ] `cd src-tauri && cargo clippy -p trigger-clock --all-targets -- -D warnings` exits 0
- [ ] `cd src-tauri && cargo fmt -p trigger-clock --check` exits 0
- [ ] `node plans/_tools/check-crates.mjs` exits 0. Warnings about template text (`still has template code`) are expected until the glue card in slice C replaces `start`; they do not fail the check.
- [ ] `git status --porcelain` lists only paths under `files:`

## Out of scope

- `daily-first-use`: T07.
- Persisting anything across restarts.
- Changing `JUMP`, `TICK` or `NORMAL_FOR`.
- Editing `lib.rs`.

## If stuck

- If pasted code does not compile or clippy objects, check that you pasted it exactly and in the place the step names. If it still fails, set `status: blocked` and quote the compiler error. Do not change the code.
- If a test from T05 now fails, you deleted too much or too little in step 1: restore `logic.rs` from `git diff` and redo it. If it still fails, set `status: blocked`.
- If `cargo fmt -p trigger-clock --check` fails, run `cd src-tauri && cargo fmt -p trigger-clock` once and carry on.
- Anything else: set `status: blocked`, add a Log line saying what you saw, and stop.

## Log

<!-- one line per attempt: date, model, result, short note -->
- 2026-10-12 haiku: built, acceptance passed
- 2026-10-12 checker: pass

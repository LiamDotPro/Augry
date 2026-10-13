---
id: R03
epic: 007-trigger-clock
question: Does a thread sleeping across a system sleep wake with the right elapsed time on each OS, or must the crate re-check the wall clock on a short tick
status: answered
---

## Answer

No. Std waits (`thread::sleep`, `recv_timeout`, `Condvar::wait_timeout`) are measured on clocks that stop during suspend on Linux and macOS, and on Windows (8 and newer) the underlying relative timers stop too. A thread told to sleep 6 hours will wake about 6 hours of awake time later, not at the wall clock time.
So the clock thread must wake on a short tick (30 s), read `Clock::now()` (wall clock) each time, and compare it with the previous tick. A wall gap of more than 90 s means "the machine slept" (or the process was frozen). Use that for `catch_up` and `daily-first-use`.
Keep the loop as a pure `step(state, now)` function so FixedClock tests drive sleeps by jumping the clock.

## Evidence

Std source, read from rust-lang/rust master on 2026-10-12 (local rustc is 1.99.0):

- Linux: `library/std/src/sys/time/unix.rs` line 70: `CLOCK_ID = libc::CLOCK_MONOTONIC` for non Apple unix. CLOCK_MONOTONIC does not count suspend (man clock_gettime(2): "does not count time that the system is suspended"; CLOCK_BOOTTIME does). `library/std/src/sys/thread/unix.rs` `sleep` (line 553 onward) calls `clock_nanosleep(Instant::CLOCK_ID, 0, ...)` on linux, so sleep is on CLOCK_MONOTONIC. The code comment says so explicitly (lines 569 to 577). Condvar (`sys/sync/condvar/futex.rs`, `futex_wait` with a timeout) uses futex relative timeouts, which are CLOCK_MONOTONIC. `sys/pal/unix/sync/condvar.rs` line 149 sets `CLOCK = CLOCK_MONOTONIC` for the pthread variant.
- macOS: same file lines 55 to 67: std uses `CLOCK_UPTIME_RAW` on Apple, with the comment "does not increment while the system is asleep ... based on mach_absolute_time, which is the clock that all timeouts and deadlines are measured against inside the kernel". `thread::sleep` on macOS goes through nanosleep, which the same comment says is tied to that clock. `Condvar` uses pthread_cond_timedwait_relative_np (relative, same kernel clock).
- Windows: `sys/thread/windows.rs` lines 117 to 131: `sleep` uses a high resolution `WaitableTimer` with a relative duration, falling back to `Sleep`. Microsoft docs for SetWaitableTimer (read 2026-10-12): "Windows 8 and newer: If relative time is specified, the timer does not include time spent in low-power states. For example, the timer does not continue counting down while the computer is asleep." (Windows 7 and older did count it.) `Instant` uses QueryPerformanceCounter, which Microsoft says includes time in standby, hibernate and connected standby. So on Windows `Instant` advances over sleep while the wait timers do not. The two disagree, so never use `Instant` to detect sleep. Condvar/recv_timeout use WaitOnAddress with a relative timeout; I did not find a doc line for its sleep behaviour, so treat it like the timer (stops).
- `std::time::Instant` docs (`library/std/src/time.rs` lines 56 to 57): "it is also not specified whether system suspends count as elapsed time or not. The behavior varies across platforms and Rust versions." So even the above can change; the design must not depend on it.
- Not run: I cannot close a lid in this environment, so none of this was tested live on suspend. The conclusions rest on source and OS docs.

Consequence on wake: all three OSes, a wait of 30 s that straddles a 5 hour sleep ends about 30 s of awake time after resume at worst (it may end at once if the remaining time had already elapsed on the stopped clock, never earlier than that). So a 30 s tick bounds lateness after wake to about 30 s.

Repo: `src-tauri/crates/trigger-core/src/testkit.rs` has `trait Clock: Send + Sync { fn now(&self) -> SystemTime }`, `SystemClock`, and `FixedClock(pub Mutex<SystemTime>)` (lines about 66 to 90). `RecordingSink::wait_for` uses `Instant` for its own timeout, which is fine for tests.

## Implications for the cards

Loop design (slice C), all constants named in one place:

```rust
const TICK: Duration = Duration::from_secs(30);       // max wait between wall clock checks
const JUMP: Duration = Duration::from_secs(90);       // wall gap above this = machine slept / process frozen (3 ticks)
const NORMAL_FOR: Duration = Duration::from_secs(60); // daily-first-use needs this much normal time
```

1. The thread owns `State { prev_wall: SystemTime, normal_since: SystemTime, cursors per rule, last_daily: Option<NaiveDate> }`. It blocks on `rx.recv_timeout(wait)` where `rx` is the stop/reload channel (so stop and rule reload are immediate), with `wait = min(TICK, until_next_due)`. Never wait longer than TICK, even if the next due time is days away.
2. After every wake (timeout or message) call `clock.now()` once and run the pure function `step(&mut State, now: SystemTime, rules: &[Rule]) -> Vec<Fire>`. Do not use `Instant` anywhere in the decision.
3. In `step`: `gap = now.duration_since(prev_wall)`.
   - `Err` (wall clock went backwards, NTP or manual): treat as no jump, set `prev_wall = now`, reset `normal_since = now`, fire nothing this step.
   - `gap > JUMP`: slept. Set `normal_since = now`. For each schedule rule: if `catch_up`, find the latest due time in `(prev_wall, now]` and fire once (not once per missed slot); otherwise just move the cursor to `now`, firing nothing for the missed slots. Intervals: same, one catch up fire at most. Add the due time and a `caught_up: true` field to the payload.
   - `gap <= JUMP`: normal. Fire every rule whose `next_due(cursor) <= now`, then advance the cursor.
   - Always end with `prev_wall = now`.
4. `daily-first-use` (Decision 5): in `step`, after the above, if `local_date(now) != last_daily` and `now - normal_since >= NORMAL_FOR`, fire and set `last_daily = local_date(now)`. At process start set `normal_since = now` and `prev_wall = now`. Result: a machine that slept all night wakes, the first step sees the jump and resets `normal_since`, the step about 60 s later (2 ticks) fires. A jump at 23:59 followed by normal ticks past midnight also fires correctly because the date check is on `now`. Use the same local time zone source as the cron card for `local_date`.
5. Testing: `step` takes `now` as a plain argument; the thread wrapper calls `clock.now()`. Tests build `State`, then call `step` with times: normal ticks 30 s apart, then a 6 hour jump, asserting catch_up on/off, daily-first-use not firing on the jump step but firing 60 s later, backward jump firing nothing. The wrapper test uses `Arc<FixedClock>` plus a short `TICK` injected through a `ClockConfig { tick, jump }` field (default the constants above) so the live test finishes in milliseconds; advance the FixedClock inside the test to simulate a sleep.
6. Do not add a platform power notification hook (WM_POWERBROADCAST, IOKit, logind). The tick makes it unnecessary and keeps the crate free of OS code.

## Doubts

- No real suspend test was run. Windows Condvar/WaitOnAddress behaviour across sleep is inferred, not documented. The design is safe either way because it never trusts the wait length.
- Modern Standby (connected standby) laptops can run timers briefly while the lid is shut; a tick can fire during a "sleep". The wall gap logic still works because the gap is measured, not assumed.
- Whether `last_daily` should survive an app restart (so a restart at 10:00 does not fire a second time) is a product decision; the loop above keeps it in memory only. Persisting needs a state file, not covered here.
- Std behaviour is explicitly unspecified and may change between Rust versions, which is another reason to use the tick.

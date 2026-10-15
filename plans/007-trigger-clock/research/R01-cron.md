---
id: R01
epic: 007-trigger-clock
question: Current, maintained cron parsing crate with local time zone support and its version
status: answered
---

## Answer

Use `croner = "4.0.1"` (default `chrono` feature) with `chrono = "0.4.45"`. `chrono` 0.4.45 is already in `src-tauri/Cargo.lock`, so only croner, `derive_builder` and `strum` are new.
Build the parser with `CronParser::builder().seconds(Seconds::Disallowed).year(Year::Disallowed).build()` to force exactly five fields.
Call `cron.find_next_occurrence(&now_local, false)` with a `chrono::DateTime<Local>`. DST is handled (details below).
Local offset: `chrono::Local` (no `time` crate, and `chrono-tz` only as a dev dependency for tests).

## Evidence

Registry (crates.io API, read 2026-10-12):

| Crate | Latest | Last release | Notes |
|---|---|---|---|
| croner | 4.0.1 | 2026-10-02 | 10M downloads, five fields native, chrono or jiff |
| cron | 0.17.0 | 2026-06-18 | Seconds field required (six or seven fields), would need a `0 ` prefix hack |
| saffron | 0.1.0 | 2021-02-01 | Abandoned, rejected |
| chrono | 0.4.45 | 2026-06-04 | already in Cargo.lock at 0.4.45 (line 521) |
| time | 0.3.55 | 2026-08-01 | already in lock at 0.3.55 |

Dependencies of croner 4.0.1 (cargo metadata): features `default = [chrono]`, `jiff`, `serde`. Non optional: `derive_builder ^0.20.2`, `strum ^0.27.2`. `chrono-tz` is a dev dependency only. `cargo tree` in a scratch project showed croner pulling `chrono`, `derive_builder` (proc macro) and `strum`. The repo lock already has `chrono`, `iana-time-zone 0.1.65` (line 1764) and `phf`, but not `derive_builder` or `strum`. No Cargo.toml in `src-tauri/` or `crates/*` names chrono directly today (grep); it arrives transitively.

Ran in a scratch project (croner 4.0.1, chrono 0.4.45, chrono-tz 0.10.4), zone America/New_York:

```
0 9 * * 1-5 from Fri 2026-03-06 08:00 EST
  2026-03-06 09:00 EST, 2026-03-09 09:00 EDT, 2026-03-10 09:00 EDT   (weekend skipped, 09:00 local across DST)
*/15 * * * * from 2026-03-06 08:00 EST
  08:15, 08:30, 08:45
Spring forward (2026-03-08 02:00 -> 03:00):
  30 2 * * *   from 03-07 23:50: 03-08 03:00 EDT (skipped 02:30 runs at 03:00), then 03-09 02:30 EDT
  0 9 * * *    from 03-07 23:50: 03-08 09:00 EDT, 03-09 09:00 EDT
  */30 * * * * from 03-08 01:00 EST: 01:30 EST, 03:00 EDT, 03:30 EDT, 04:00 EDT
Fall back (2026-11-01 02:00 EDT -> 01:00 EST):
  30 1 * * *   from 10-31 23:50: 11-01 01:30 EDT (once, first occurrence), 11-02 01:30 EST
  0 * * * *    from 11-01 00:30 EDT: 01:00 EDT, 01:00 EST (fires twice), 02:00 EST, 03:00 EST
```

Also ran with `chrono::Local` and `TZ=America/New_York cargo run`: `0 9 * * 1-5` from 2026-03-06 08:00 -05:00 gave 03-06 09:00 -05:00, 03-09 09:00 -04:00, 03-10 09:00 -04:00. So `Local` honours the process time zone on macOS, and `TZ` is a handy override for live tests on unix.

Five field enforcement: the default `Cron::from_str` accepts six fields (seconds optional) and seven (year). `"0 0 9 * * 1-5"` parsed Ok with the default. With `Seconds::Disallowed` it returns `Err(InvalidPattern("Pattern must have 5 fields when seconds are disallowed."))` and `"0 9 * * 1-5"` is Ok. (I only ran the Seconds setting; the `Year::Disallowed` call is from the source, parser.rs lines 63 to 70.)

Source read: `~/.cargo/registry/src/*/croner-4.0.1/src/parser.rs` lines 54 to 100 (`Seconds`, `Year`, `CronParser` builder), `lib.rs` line 320 (`find_next_occurrence`), 637 to 667 (`iter_from`, `iter_after`).

## Implications for the cards

`src-tauri/crates/trigger-clock/Cargo.toml`:

```toml
[dependencies]
croner = "4.0.1"
chrono = "0.4.45"
[dev-dependencies]
chrono-tz = "0.10.4"
```

Code:

```rust
use croner::{Cron, parser::{CronParser, Seconds, Year}};
use chrono::{DateTime, Local, TimeZone};

let parser = CronParser::builder()
    .seconds(Seconds::Disallowed)
    .year(Year::Disallowed)
    .build();
let cron: Cron = parser.parse("0 9 * * 1-5")?;                              // Err for 6 or 7 fields
let next: DateTime<Local> = cron.find_next_occurrence(&now_local, false)?;  // false = strictly after now
```

- `find_next_occurrence` is generic over `chrono::TimeZone`. In production pass `Local.timestamp_opt(secs, 0).unwrap()`. In unit tests pass `chrono_tz::America::New_York.with_ymd_and_hms(...)` so DST tests do not depend on the machine zone. Have the `Clock` trait return a UTC instant and convert inside `next_due`.
- Make `next_due` generic over `Tz: TimeZone` (or take the zone as a parameter) so tests inject `New_York`.
- Validate the expression at rule `check` time with `parser.parse` and report its error text.
- DST behaviour to put in the README and pin with tests (expected values above): a time skipped by spring forward runs at the first valid instant after the gap; a fixed time in a repeated hour (`30 1`) runs once; an every hour pattern (`0 * * * *`) fires at both 01:00 EDT and 01:00 EST.
- Local offset: use `chrono::Local` (links `iana-time-zone`, already in the lock). Avoid `time` with `local-offset`: it is known to refuse to read the offset in multithreaded Unix programs, and the trigger runs on its own thread. This is from memory of the `time` docs, not re-read.

## Doubts

- Reduced chrono features were not tried. croner pulls chrono with default features anyway, so plain `chrono = "0.4.45"` is the simple card.
- The `time` crate local offset behaviour was not re-verified today.
- Double firing of hourly patterns on fall back is croner's behaviour. If the epic wants once only, `next_due` needs a dedupe against the previous fire instant.
- `plans/_tools/check-crates.mjs` was not read, so whether it audits dependencies is unknown.
- croner 4.0.1 is 10 days old; a 4.0.x change before the build is possible.

---
id: R02
epic: 002-trigger-internal
question: Current `regex` crate version and its size cost in the app binary
status: answered
---

## Answer

`regex` 1.13.1 (latest stable on crates.io, 2026-07-15) is already in `src-tauri/Cargo.lock` at exactly 1.13.1 with default features, pulled in by `tauri-utils` and `urlpattern`. Adding it directly costs 0 bytes. Put `regex = "1.13"` in `[workspace.dependencies]` and `regex = { workspace = true }` in the app. No ANSI stripper crate is in the lock (no `vte`, `vt100`, `strip-ansi-escapes`), so write a small stripper. Build with `RegexBuilder::new(p).case_insensitive(true).size_limit(256 * 1024)`, match per line.

## Evidence

- crates.io API, read 2026-10-12: max stable `regex` 1.13.1.
- `src-tauri/Cargo.lock` lines 3278-3304: `regex 1.13.1`, `regex-automata 0.4.18`, `regex-syntax 0.8.11`, `aho-corasick 1.1.5`, `memchr 2.8.3`. Dependents: `tauri-utils` (line ~4430) and `urlpattern` (line ~5034); `globset` uses only regex-automata and regex-syntax.
- `cargo tree -e features -i regex --offline` in `src-tauri`: `regex feature "default"` is enabled via `tauri-utils 2.9.3`, which `tauri` links at runtime, so the code is already in the binary with unicode and perf features. A direct dependency with default features changes nothing.
- grep for `vte|vt100|strip-ansi|anstream|anstyle-parse|termwiz|vtparse` in the lock: no matches. `src/pty/reader.rs` has no ANSI handling (only the coalescer); `pty/shell.rs` is the shell integration.
- Scratch test (regex =1.13.1, release, run here):
  - `RegexBuilder::new(r"(\w{50}){50}").size_limit(262144).build()` fails with "Compiled regex exceeds size limit of 262144 bytes." (default limit is 10 MiB).
  - `\w{100}` with Unicode also exceeds 256 KiB; a normal pattern like `error: .*failed` compiles fine.
  - Pattern `(a+)+b|error.*(failed|denied)` on a 4096 byte `"a"` buffer: 1000 `is_match` calls took 1.7 ms total (about 1.7 microseconds each). No catastrophic backtracking (the crate is automata based, linear time).
  - `error|panicked at` on 60 lines: 1000 calls took 0.18 ms.

## Implications for the cards

- `src-tauri/Cargo.toml`: under `[workspace.dependencies]` add `regex = "1.13"`; under the app `[dependencies]` add `regex = { workspace = true }`. The lock stays at 1.13.1; only the `terminal-grid` dependency list gains `regex`.
- Compile once per rule, at rule load, not per chunk:
  ```rust
  regex::RegexBuilder::new(&pattern)
      .case_insensitive(true)
      .size_limit(256 * 1024)
      .build()
  ```
  A build error (syntax or size) must reject the rule with the error text, not panic. Leave Unicode on (default).
- Bounding cost: keep per pane a buffer capped at 4 KiB, trimmed from the front on a char boundary. Strip ANSI first, split on `\n` and `\r`, run `is_match` on each complete line plus the trailing partial line, and only on a match take the line truncated to 200 chars for the payload. Scan only panes with an armed `tg.output-matched` rule (the epic Risk). Search is linear in input, so 4 KiB per chunk is microseconds.
- After a match, drop the buffer up to the end of the matching line so the next chunk does not re-fire on the same text.
- ANSI stripper (about 40 lines in `logic.rs`, pure and tested): byte state machine that drops CSI (`ESC [` to a final byte `0x40..=0x7E`), OSC (`ESC ]` to `BEL` or `ESC \`, which covers the shell integration marks), and other two byte `ESC x` escapes. Keep an incomplete escape state per pane across chunks, then `String::from_utf8_lossy`. Tests: colour codes, an OSC mark, an escape split across two chunks, a lone ESC at the end.
- No new crate for stripping, so `regex 1.13` is the only new dependency decision.

## Doubts

- Absolute regex size in the release binary was not measured since it is already linked; the marginal cost is zero by the feature tree. No `cargo bloat` figure.
- The 256 KiB limit comes from one scratch test; a card could lower it to reject heavy Unicode classes earlier.

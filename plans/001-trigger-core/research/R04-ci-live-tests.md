---
id: R04
epic: 001-trigger-core
question: Do GitHub's macOS, Windows and Ubuntu runners deliver file events to `notify` quickly enough for live tests with a 2 s timeout, and is the inotify watch limit on Ubuntu a problem?
status: partial
---

## Answer

Yes in practice, with margin: use a poll-until-deadline helper with a 10 s ceiling (not a single 2 s `recv_timeout`), keep the live tests to about 4 to 6, each with its own temp dir and one watcher. The inotify limit is not a problem for that: each test needs a handful of watches. macOS FSEvents is the only real flake risk (Zed and Bazel both hit it), so canonicalise the temp path and allow a 500 ms settle after `watch()` before touching files. The exact runner `max_user_watches` value is not published and was not measured (see Doubts).

## Evidence

Read 2026-10-12. I could not run GitHub runners from here, so this is read evidence, not a run.

- notify's own CI (https://raw.githubusercontent.com/notify-rs/notify/main/.github/workflows/ci.yml): runs `cargo test --workspace --exclude examples` on ubuntu-latest and windows-latest (stable, nightly, 1.88) and on macos-latest with both `macos_fsevent` and `macos_kqueue`. No retries, no `timeout-minutes`, no flake handling. So notify's real-watcher tests run unmodified on all three hosted runners and pass routinely.
- notify releases (https://github.com/notify-rs/notify/releases): latest stable is 8.2.0 (3 Aug 2025); 9.0.0 is only at rc. 8.2.0 added a notice when inotify `max_user_watches` is hit, so the failure is reported as an error rather than silent.
- Flakiness elsewhere, all on macOS FSEvents:
  - Zed raised its fs-event test timeout from 2 s to 4 s on CI (https://git.secluded.site/zed/commit/c1d9dc369c1edcae0b3f84bdabc2bdf73d5c2e72), then disabled those tests as "unresolvably flaky" (https://git.secluded.site/zed/commit/585ee6169e8703720c2b2ae47adb895df4ac080e).
  - Bazel re-enabled fsevents tests, saw them flake again, and moved to retrying a query for several seconds (https://bazel.googlesource.com/bazel/+/c5690fefe2d0796b8793ebd16db79fca34b6171c).
  - MongoDB Rust driver disabled timing based tests on macOS CI as hosts are slow (https://jira.mongodb.org/browse/RUST-2342).
  - FSEvents coalesces by design: the stream latency is a wait before delivery, so a fixed short timeout is fragile on a loaded host.
- inotify limit: not documented for GitHub runner images. A web search found only generic Ubuntu numbers (8192 on old Ubuntu, 65536 or more on newer kernels, 524288 as the common manual raise). Not a measured runner value.
- Temp dirs: `std::env::temp_dir()` is `/tmp` (ext4, inotify works) on Linux, `/var/folders/...` on APFS on macOS (FSEvents works, but `/var` is a symlink to `/private/var`, and events report the `/private/var` path), and a path under `C:\Users\runneradmin\AppData\Local\Temp` (NTFS, ReadDirectoryChangesW works) on Windows. All local disks, none network or tmpfs-without-events. This is from knowledge of the images, not measured.
- This repo: `.github/workflows/ci.yml` has one `test` job, matrix `[windows-latest, macos-latest, ubuntu-22.04]`, `fail-fast: false`, Linux webkit deps already installed, last step `cargo test` with `working-directory: src-tauri` (lines 47 to 49 of the file as read). No clippy or fmt step yet.

## Implications for the cards

Test design (slice with the live tests, all `#[ignore]`):
- Do not use a bare 2 s `recv_timeout`. Write one helper `wait_for(rx, pred, Duration::from_secs(10))` that loops `recv_timeout(100 ms)` until the predicate matches or the deadline passes. Local runs still return in tens of ms; the ceiling only matters on a slow runner.
- Negative assertions ("no event for a non matching file") must use a short fixed quiet window (500 ms to 1 s) after a positive event for a sentinel file has arrived, never a bare sleep.
- Create the temp dir with `tempfile::tempdir()` and pass `dir.path().canonicalize()?` to the watcher and to expected paths. This avoids the macOS `/var` vs `/private/var` mismatch (and the Windows `\\?\` prefix: compare canonicalised on both sides).
- After `watch()` returns, sleep 300 to 500 ms before the first file operation (FSEvents stream start up is asynchronous; Bazel and Zed both saw lost early events).
- One watcher per test, dropped at test end, no shared dirs. Keep to 4 to 6 live tests (create, modify, delete, glob exclude, atomic save, rapid burst debounce). Run them with `-- --ignored --test-threads=1` to avoid cross test CPU contention on 2 to 4 core runners.
- Atomic save and rename details depend on R03.

ci.yml changes (all inside the existing `test` job, `working-directory: src-tauri`). Add components to the toolchain step, then replace the final `Core tests` step:

```yaml
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
```

```yaml
      - name: Format check
        run: cargo fmt --check
        working-directory: src-tauri

      - name: Clippy
        run: cargo clippy -p trigger-core -p trigger-hub -p trigger-fs --all-targets -- -D warnings
        working-directory: src-tauri

      - name: Core tests
        run: cargo test --workspace
        working-directory: src-tauri

      - name: Inotify limits (Linux, for the log)
        if: runner.os == 'Linux'
        run: cat /proc/sys/fs/inotify/max_user_watches /proc/sys/fs/inotify/max_user_instances

      - name: Live file watching tests
        run: cargo test -p trigger-fs -- --ignored --test-threads=1
        working-directory: src-tauri
        timeout-minutes: 10
```

Notes:
- Keep the existing Linux apt step first so `--workspace` and clippy can build the Tauri crate.
- Fmt and clippy run on all three OSes via the matrix; that is fine and costs little with rust-cache. If time matters, gate them with `if: runner.os == 'Linux'`.
- No retry action. If macOS flakes after the above, the fallback is a single `nick-fields/retry` style rerun on macOS only, not a longer timeout.
- No sysctl change is needed. If the log step ever shows a value under 8192, add `sudo sysctl -w fs.inotify.max_user_watches=65536` before the live step (runners have passwordless sudo).

## Doubts

- The actual `fs.inotify.max_user_watches` on `ubuntu-22.04` runners was not measured and is not documented. The log step above records it on first run. Even at 8192, 6 tests each watching a tiny temp dir use well under 100 watches.
- No first hand measurement of FSEvents latency on `macos-latest` (arm64). The 10 s ceiling and 500 ms settle are drawn from other projects' experience, not from a run. First CI run on a draft PR should confirm.
- `src-tauri` workspace layout and whether `tempfile` is already a dependency (dev-dependency needed) were not checked.

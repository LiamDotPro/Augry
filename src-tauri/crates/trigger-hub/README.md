# trigger-hub

Runs trigger sources: keeps the rule registry, starts and stops sources so
that exactly the applied rules are live, cools down noisy rules and fans fired
events out to subscribers. Design: `docs/process-triggers.md` Part 2.

## Develop

Run from `src-tauri`:

- `cargo test -p trigger-hub`
- `cargo clippy -p trigger-hub --all-targets -- -D warnings`
- `cargo fmt -p trigger-hub --check`

From the repo root: `node plans/_tools/check-crates.mjs`.

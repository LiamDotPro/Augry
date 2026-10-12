# trigger-core

The contract between trigger sources and whatever runs them (types, the
`Source` trait) and a small test kit. Design: `docs/process-triggers.md`
Part 2.

## Develop

Run from `src-tauri`:

- `cargo test -p trigger-core`
- `cargo clippy -p trigger-core --all-targets -- -D warnings`
- `cargo fmt -p trigger-core --check`

From the repo root: `node plans/_tools/check-crates.mjs`.

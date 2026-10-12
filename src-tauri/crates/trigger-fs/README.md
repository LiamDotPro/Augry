# trigger-fs

Fires when files change under a watched folder.

Source id `fs`. One crate, one source: everything it fires starts with
`fs.`.

## Kinds

<!-- kinds -->
| Kind | Title | Platforms | Permission | When | Payload |
| --- | --- | --- | --- | --- | --- |
| fs.example | Example | macOS, Windows, Linux | none | example* | example |
<!-- /kinds -->

## Rule fields

The `when` object of a trigger, parsed by `src/rule.rs`. Unknown fields are an
error, so a typo shows up in the editor.

| Field | Required | Default | Meaning |
|---|---|---|---|
| `example` | yes | | Replace with the real fields |

## Payload

What an event carries, available to steps as `${trigger.<field>}`. Paths, names,
ids and numbers only. Never file contents or secrets, never more than 4 KiB.

| Field | Meaning |
|---|---|
| `example` | Replace with the real fields |

## Platforms

| Platform | How it is detected | Limits |
|---|---|---|
| macOS | | |
| Windows | | |
| Linux | | |

## Permissions

What the OS asks the user for, and when. "None" is a fine answer.

## Example

```json
{ "id": "example", "kind": "fs.example", "when": { "example": "value" } }
```

## Develop

| File | What it holds |
|---|---|
| `src/lib.rs` | `KINDS`, `FsSource` and the thread that joins the OS layer to the logic. Glue only |
| `src/rule.rs` | The typed `When` and `parse`. Pure, unit tested |
| `src/logic.rs` | Pure: which rules a `Signal` fires, with what payload, and any debounce state. Unit tested |
| `src/os.rs` | The only file that talks to the OS: `availability()` and `listen()`, per platform behind `cfg` |
| `tests/live.rs` | `#[ignore]` tests against the real OS |
| `tests/readme.rs` | Fails when the Kinds table above is stale |

```
cd src-tauri
cargo test -p trigger-fs                       # pure and readme tests, any OS, offline
cargo test -p trigger-fs -- --ignored          # live tests on this machine
cargo clippy -p trigger-fs --all-targets -- -D warnings
```

## Known limits

What does not work, on which platform, and why.

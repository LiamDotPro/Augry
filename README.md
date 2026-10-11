# Augry

[![Latest release](https://img.shields.io/github/v/release/LiamDotPro/Terminal-Grid?label=release)](https://github.com/LiamDotPro/Terminal-Grid/releases/latest)
[![CI](https://github.com/LiamDotPro/Terminal-Grid/actions/workflows/ci.yml/badge.svg)](https://github.com/LiamDotPro/Terminal-Grid/actions/workflows/ci.yml)
[![Downloads](https://img.shields.io/github/downloads/LiamDotPro/Terminal-Grid/total)](https://github.com/LiamDotPro/Terminal-Grid/releases)
![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-informational)
[![Built with Tauri 2](https://img.shields.io/badge/built%20with-Tauri%202-24C8DB?logo=tauri&logoColor=white)](https://tauri.app)

Run several coding agents side by side. Augry (formerly Terminal Grid) puts
your terminals in a grid, tells you when an agent finishes, lets you review
and comment on its changes, and keeps your notes next to the work.

![Four panes in a 2x2 grid: tests, git history, Claude Code with its task line, and an idle pane with agent launchers](docs/screenshots/grid.png)

- **A grid of terminals**: up to 3x3 per page, as many pages as you need.
  Stack them as a grid, side by side or on top of each other, and drag the
  gaps to resize.
- **Knows your repo**: every pane shows its repository and branch, with a menu
  to switch, create and remove worktrees.
- **Knows your agents**: a pane lights up while Claude Code, Codex, Gemini,
  Aider and others run, and turns green when they finish. An idle pane offers
  a button to start each agent on your PATH.
- **See what an agent is doing**: agents can write their current task to a
  file, and the pane shows it under the header.
- **Review and request changes**: put a pane next to its repository's
  changes, comment on lines, edit any file, then send every comment to the
  agent in one go.
- **Notes**: select text in a terminal to save it as a markdown note linked to
  that session, or write your own. Autosaved, with a live preview.
- **Keyboard first**: everything has a hotkey; hold the modifier to see them.
- **Your look**: System, Light, Dark and Black themes (plus Glass on macOS), a
  compact layout, and your own focus and finished colours. See
  [Appearance](docs/wiki/Appearance.md).

![The Notes tab with a release checklist open next to its preview](docs/screenshots/notes.png)

## Install

Download the latest build from the
[Releases page](https://github.com/LiamDotPro/Terminal-Grid/releases/latest).

- **Windows** (10 1809 or later): `…_x64-setup.exe` installs for you without
  an admin prompt; `…_x64_en-US.msi` is for scripted or per machine installs.
  Both install WebView2 if it is missing.
- **macOS**: `…_universal.dmg`, signed and notarized, native on Apple Silicon
  and Intel.
- **Linux**: `…_amd64.AppImage` (`chmod +x` and run), `…_amd64.deb`
  (`sudo apt install ./<file>.deb`) or `…x86_64.rpm`. Needs WebKitGTK 4.1,
  which the packages pull in.

On Windows the app starts `pwsh`, then Windows PowerShell, then `%COMSPEC%`;
elsewhere `$SHELL`, then `/bin/bash`. You can pick another shell in Settings.
PowerShell, bash and zsh get a small integration script so pane labels follow
`cd` and finished agents are noticed straight away; other shells rely on
process detection.

More guides are in the [docs wiki](docs/wiki/Home.md).

## Hotkeys

Windows and Linux use `Ctrl+Alt`, or `Ctrl+Shift` if you pick it in Settings
(then `Alt` replaces `Shift` below). macOS uses ⌘, which never reaches the
shell. Hold the modifier to see this list in the app.

| Windows / Linux | macOS | Action |
|---|---|---|
| `<mod>+N` | `⌘N` | New pane (pick a folder) |
| `<mod>+Shift+N` | `⌘T` | New pane in the same folder |
| `<mod>+L` | `⌘L` | Cycle stacking: grid, side by side, stacked |
| `<mod>+W` | `⌘W` | Close pane (asks first while an agent runs) |
| `<mod>+R` | `⌘R` | Restart the shell |
| `<mod>+←↑↓→` | `⌃⌘←↑↓→` | Move the focused pane |
| `Alt+←↑↓→` | `⌥⌘←↑↓→` | Focus the neighbouring pane |
| `<mod>+1…9` | `⌘1…9` | Focus pane n on the page |
| `<mod>+[` / `]` | `⇧⌘[` / `]` | Previous / next page (also `PageUp` / `PageDown`) |
| `<mod>+Enter` | `⌘↩` | Review the focused pane, and back |
| `<mod>+S` | `⌘S` | Save the selection as a note |
| `<mod>+Tab` | `⌃Tab` | Next tab |
| `<mod>+B` / `P` | `⌘B` / `P` | Notes: show or hide the list / the preview |
| `<mod>+,` | `⌘,` | Settings |
| `F11` | `⌃⌘F` | Full screen |

Copy and paste work as usual: `⌘C` / `⌘V` on macOS; elsewhere `Ctrl+C`
copies while text is selected, and `Ctrl+V` or `Shift+Insert` pastes.

## Panes

The switch in the top bar (or `<mod>+L`) stacks a page as a **grid**, **side
by side** or **stacked**. Drag the gap between two panes to resize them;
double-click it to even them out. A pane's `+` button opens a new pane right
after it.

**Agents.** A pane counts as running an agent while a process in its shell
matches one of the name patterns in Settings. When the agent exits, the pane
turns green until you look at it. For Claude Code's bell to count too, run
`claude config set --global preferredNotifChannel terminal_bell`.

**Current task.** Every shell gets a `TERMINAL_GRID_TASK_FILE` variable;
whatever is written to that file shows under the pane header within a second:

```
echo "Adding retries to the upload client" > "$TERMINAL_GRID_TASK_FILE"
```

The `task` button can **Ask agent** to write it now, or **Copy instructions**
for your `CLAUDE.md` / `AGENTS.md` so the agent keeps it up to date.

**Worktrees.** A fork icon before the branch marks a linked worktree. Click
the branch to see the repository's worktrees: picking one moves the pane
there, jumps to a pane already in it, or opens a new pane if an agent is
running (Shift+click always does). **New worktree…** creates one next to the
main checkout. Unused worktrees can be removed; the branch stays.

## Review

![Review: Claude Code with a saved session note below it, and the review panel's Edit mode showing the repository tree and a changed file](docs/screenshots/review.png)

`<mod>+Enter` puts the focused pane next to a review panel for its
repository. The panel lists **Staged** and **Unstaged** files with stage and
unstage buttons. Comment on a staged line (click its number or press `C`, then
`Ctrl+Enter` / `⌘↩`), and **Request changes** sends every pending comment,
with an optional note, to the agent.

| Key | In the panel |
|---|---|
| `↑` / `↓` | Previous / next file |
| `N` / `Shift+N` | Next / previous change |
| `F` | Changes only or the whole file |
| `E` | Edit mode; `Ctrl+S` (`⌘S`) saves, `Esc` hands the keys back |
| `C` | Comment on the current line |

Edit mode shows the whole repository as a tree (**All files** or
**Changed**) and lets you edit any file. Added, modified and unsaved lines are
marked, and `Alt+↑` / `Alt+↓` step through changed files.

## Notes

![A line selected in Claude Code with the Save to note and Copy menu next to it](docs/screenshots/save-to-note.png)

Plain `.md` files in a folder you choose (default `Documents/TerminalGrid/Notes`),
saved as you type, with a prompt if a file changes outside the app. Deleted
notes go to the trash.

Select text in a terminal, including inside Claude Code, and pick **Save to
note** or **Copy**. A saved note goes into a folder named after the repo,
titled from its first line and linked to that pane, with **Undo** for a few
seconds. While reviewing a pane, its notes show below the terminal: **This
session**, or **All in** the repo, with **+ Note** to write one.

## Development

Needs Rust stable, Node 22 and Git, plus:

- **Windows**: `rustup default stable-msvc` and Visual Studio Build Tools
  ("Desktop development with C++").
- **macOS**: `xcode-select --install`.
- **Linux** (Debian/Ubuntu):
  `sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`

```
npm install
npm run tauri dev            # run
npm run tauri build          # installers in src-tauri/target/release/bundle/
npm test                     # frontend tests
cd src-tauri && cargo test   # Rust tests
```

`src/` is the frontend (Vite, React 19, xterm.js) and `src-tauri/` the Rust
core. The design is in `docs/technical-design.md` and the contract between the
two in `src/ipc/types.ts`.

### Release

Bump the version in `package.json`, `src-tauri/Cargo.toml` and
`src-tauri/tauri.conf.json`, commit, and push an annotated tag; its message
heads the release notes:

```
git tag -a v0.6.0 -m "Augry 0.6.0" -m "- What changed"
git push origin v0.6.0
```

[`release.yml`](.github/workflows/release.yml) builds every platform and
publishes the release once all of them succeed.
[`ci.yml`](.github/workflows/ci.yml) tests every push and pull request. macOS
signing is in `docs/macos-signing.md`, store publishing in
`docs/store-publishing.md`.

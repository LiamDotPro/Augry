# Process triggers

Things on the desktop that could start a process, and how the app is built to
notice them.

**Part 1** is the catalogue: a long list of ideas, grouped by which platforms
support them. Each trigger names an example process and how the app could notice
it, so the list doubles as a rough feasibility check.

**Part 2** is the design: every integration is its own Rust crate with its own
tests, built against one small contract, and planned and built with the pipeline
in `plans/`.

Part 1 sections go from widest to narrowest support:

1. [All platforms](#all-platforms)
2. [macOS and Windows](#macos-and-windows)
3. [macOS and Linux](#macos-and-linux)
4. [Windows and Linux](#windows-and-linux)
5. [macOS only](#macos-only)
6. [Windows only](#windows-only)
7. [Linux only](#linux-only)
8. [Permissions and caveats](#permissions-and-caveats)

Part 2:

9. [Design: one crate per integration](#design-one-crate-per-integration)
10. [Building it with the planning pipeline](#building-it-with-the-planning-pipeline)

# Part 1: Catalogue

"All platforms" means the trigger works on macOS, Windows and Linux, even if each
one detects it a different way. Linux notes say when X11 and Wayland differ.

Most triggers would carry a payload into the process, such as the file path, the app
name, the URL or the commit, which steps can use as `${trigger.path}` and so on.

---

## All platforms

### Inside Terminal Grid

The app already sees all of these, so they are the cheapest to build.

| Trigger | Example process | How it could be detected |
|---|---|---|
| An agent finishes in a pane | Run the tests, then open the review panel | `terminal://agent` finished event from `AgentWatcher` |
| An agent is waiting for input | Notify me, or answer with a stock reply | Agent task file line, or a known prompt pattern in the output |
| A command exits with a non-zero code | Ask an agent to explain the failure | Shell integration prompt marks with the exit status |
| A command runs longer than N minutes | Notify me when it ends | Prompt marks with timestamps |
| Output matches a pattern | `FAIL`, `panic:`, `Traceback`: send the last 50 lines to an agent | Scan output in the PTY reader |
| A terminal bell or OSC 9 / OSC 777 notification | Treat it as "this step is done" | Already parsed by `terminals/osc.ts` |
| An agent writes its task line | Record it in a session note | `terminal://task` event |
| A pane is opened in a repo | Run that repo's "start the day" process | `create_terminal` with a cwd inside a repo |
| A pane `cd`s into a repo for the first time today | Pull, install dependencies, show open PRs | Shell integration cwd reports |
| A worktree is created from the app | Install dependencies and start an agent on the branch | `worktree_create` |
| A worktree is removed | Clean up its containers and ports | `worktree_remove` |
| Changes are staged in the review panel | Ask an agent for a commit message | `review_stage` |
| A session note is created from a selection | Turn the note into a task for an agent | Notes link added |
| A note gets a tag such as `#run` or `#process:fix` | Run the named process with the note as input | Markdown parse on save |
| A checkbox in a note is ticked | Start the next step of a checklist process | Diff of the note on save |
| Focus mode is entered on a pane | Start a "deep work" process (silence, timer) | Reducer action |
| A process step finishes | Chain into another process | Run engine event |
| A process fails | Run the team's "on failure" process | Run engine event |
| The app starts | Restore yesterday's panes and rerun the morning process | App setup |
| A dev server prints "ready on http://localhost:3000" | Open the page and run a smoke check | Output pattern |

### Files and folders

Folder watching uses the `notify` crate the app already depends on: FSEvents on
macOS, ReadDirectoryChangesW on Windows, inotify on Linux.

| Trigger | Example process | How it could be detected |
|---|---|---|
| A `.md` file is saved | Lint it, rebuild the docs, or summarise the change | Watch the notes root or a repo |
| A new `.md` file appears in a folder | Treat it as a spec and start an agent on it | Watch for a create event |
| A file in an "inbox" folder appears | One process per dropped file (PDF, CSV, image) | Watch the folder |
| A download finishes | Unzip it, scan it, move it to the right project | Watch Downloads, skip `.crdownload`, `.part` and `.download` files |
| A `.zip` or `.tar.gz` lands in Downloads | Extract it and open a pane inside | Watch Downloads |
| A screenshot is taken | Attach it to an agent prompt ("what is wrong in this UI?") | Watch the screenshots folder (the path differs per OS) |
| A screen recording is saved | Transcribe it or attach it to an issue | Watch the folder |
| `package.json`, `Cargo.toml`, `go.mod` or `pyproject.toml` changes | Install dependencies, then run the tests | Watch a repo |
| A lockfile changes after a pull | Reinstall | Watch a repo |
| `.env` or `.env.example` changes | Check the two still list the same keys | Watch a repo |
| A new migration file is added | Run it against the local database and regenerate types | Watch the migrations folder |
| An OpenAPI or GraphQL schema changes | Regenerate clients (like `npm run api:generate`) | Watch the file |
| A test file is saved | Rerun only that test | Watch a repo |
| A `TODO` or `FIXME` comment is added | Open an issue for it | Diff on save |
| A log file gains a line with `ERROR` | Send the context to an agent | Tail the file |
| A crash dump or core file appears | Symbolicate it and summarise | Watch the dump folder (differs per OS) |
| A design export `.zip` lands | Run `design:sync` and "port the design changes" | Watch Downloads for a name pattern |
| A CSV export appears | Load it into a notebook or a database | Watch the folder |
| A file grows past a size | Rotate or archive it | Periodic stat |
| Disk space in a folder passes a limit | Clean `node_modules`, `target` and caches | Periodic check |
| A file is deleted | Restore it from git, or ask before it is gone for good | Watch for a remove event |
| A Dockerfile or compose file changes | Rebuild the image | Watch a repo |
| A CI workflow file changes | Lint it with `actionlint` | Watch `.github/workflows` |
| A `.ipynb` notebook is saved | Strip outputs, run it headless | Watch a repo |
| An Office or PDF document is saved | Convert it to Markdown into the notes | Watch a folder |
| Any file in a synced folder (Dropbox, Google Drive, OneDrive) changes | Pull a teammate's change into a repo | Watch the sync folder |
| A file is renamed to a magic name such as `GO.md` | Run the process the file describes | Watch for a rename |

### Git and code

| Trigger | Example process | How it could be detected |
|---|---|---|
| A local commit is made | Run the checks, ask for a review, update the changelog | Watch `.git/HEAD` and refs, or a `post-commit` hook calling the app |
| The branch is switched | Reinstall, restart the dev server, load that branch's notes | Watch `.git/HEAD` |
| A pull brings new commits | Summarise what changed since I last looked | Watch refs and `ORIG_HEAD` |
| A merge conflict appears | Start an agent on the conflict | `.git/MERGE_HEAD`, conflict markers in the status |
| A rebase stops | Explain where it stopped and why | `.git/rebase-merge` appears |
| A push is made | Open the PR page, or create a draft PR | `pre-push` hook calling the app |
| A tag is created | Run the release process | Watch `refs/tags` |
| A stash is created | Remind me about it after a day | Watch `refs/stash` |
| Uncommitted changes are older than N hours | Nudge me to commit, or offer a WIP commit | Periodic `git status` |
| The branch is behind its remote by N commits | Offer to rebase | Periodic `git fetch` |
| HEAD becomes detached | Warn me and offer to create a branch | Watch `.git/HEAD` |
| A new repo is cloned into a projects folder | Run "explore a codebase" | Watch the folder for a new `.git` |
| A repo is opened in an editor for the first time | Set up hooks and an env file | Recent projects file of the editor |
| A file changes inside a git submodule | Update the parent's pointer | Watch a repo |
| A large file is staged | Suggest Git LFS | `pre-commit` hook |
| Someone pushes to `main` (seen on fetch) | Rebase my worktrees | Periodic fetch |

### Developer tools and processes

| Trigger | Example process | How it could be detected |
|---|---|---|
| A port starts listening (3000, 5173, 8080) | Open it in the browser and start a checker | Poll the listening sockets |
| A port stops listening | The dev server crashed: show its last output, offer a restart | Same |
| A Docker container starts, stops or becomes unhealthy | Tail its logs into a pane | `docker events` stream |
| A Docker image build finishes | Run the container tests | `docker events` |
| A Kubernetes context is switched | Show a warning banner when it is production | Watch `~/.kube/config` |
| An Android emulator boots | Install the debug build | `adb devices` polling |
| An SSH session to a host is opened | Open a paired pane with logs from that host | A new `ssh` process with the host in its args |
| A package manager install finishes | Run the post install checks | The process ends, in a pane or outside |
| A test runner in watch mode reports a failure | Ask an agent to fix it | Output pattern |
| A build is slower than usual | Profile it | Timing over runs |
| A local database container becomes healthy | Run migrations and seed it | `docker events` health status |
| A language server crashes | Restart it and collect its log | The process exits |
| An editor (VS Code, a JetBrains IDE, Zed, Cursor) opens a project | Open the matching panes | A new process with the path in its args |
| An AI agent runs outside Terminal Grid (`claude` in another terminal) | Offer to pull it into a pane | Process watch, as `AgentWatcher` does now |
| A CLI tool asks Terminal Grid to run a process | `tg run review-changes` | A small CLI over a local socket |
| A git hook script calls Terminal Grid | Any hook becomes a trigger | Same CLI |
| A Makefile or npm script finishes | Chain the next process | Exit status in a pane |
| A new version of a dependency is released | Open an upgrade branch | Poll the registry |
| A new version of an agent CLI is installed | Rerun a smoke test of my processes | The binary changes on PATH |

### Apps and processes

| Trigger | Example process | How it could be detected |
|---|---|---|
| Another app starts | Slack opens: post "starting work"; Figma opens: pull the latest export | Poll the process list with `sysinfo` (already a dependency) |
| Another app quits | The editor closes: commit WIP and stop the dev servers | Same |
| An app crashes | Collect its crash log and open an issue | The process vanishes with a crash report |
| Any process uses high CPU or memory for N minutes | Show which one and offer to stop it | `sysinfo` |
| A game or full screen app starts | Pause noisy processes and notifications | Process list, or a full screen window |
| A meeting app starts (Zoom, Teams, Meet in the browser) | Silence agents, start a notes pane | Process list (the browser case needs more, see camera and mic) |
| A password manager is unlocked | Allow processes that need secrets to run | The CLI's status (for example `op whoami`) |
| A VPN client connects | Start the processes that need the company network | Process list plus the network change |
| A backup tool finishes | Check the backup and record it | Process exit or its log |
| Terminal Grid is updated | Show what changed and rerun processes that use new features | App version on start |

### System state

| Trigger | Example process | How it could be detected |
|---|---|---|
| The computer wakes from sleep | Fetch all repos, refresh PR status | macOS NSWorkspace wake notification, Windows `WM_POWERBROADCAST`, Linux logind `PrepareForSleep` |
| The computer is about to sleep | Commit WIP, pause long agents | Same signals |
| The screen is locked | Pause agents that need me, start overnight jobs | macOS `com.apple.screenIsLocked`, Windows `WTS_SESSION_LOCK`, logind `Lock` |
| The screen is unlocked | Show what happened while I was away | The matching unlock signals |
| I log in | Run the "start the day" process | Login item, Run key, or XDG autostart |
| I am idle for N minutes | Run a heavy job now (tests, builds, index) | macOS `CGEventSourceSecondsSinceLastEventType`, Windows `GetLastInputInfo`, Linux X11 screensaver or logind `IdleHint` |
| I come back after being idle | Summary of what agents did | Same, the other way |
| The laptop switches to battery | Pause heavy processes | macOS IOPowerSources, Windows power status change, Linux UPower |
| The charger is plugged in | Resume them | Same |
| The battery falls below N% | Save state, stop agents | Same |
| Light or dark mode changes | Switch the terminal theme or the doc build theme | macOS `AppleInterfaceThemeChangedNotification`, Windows `UISettings.ColorValuesChanged`, the xdg portal `color-scheme` |
| Disk space falls below N GB | Clean build caches | Periodic check |
| The time zone changes (travel) | Shift scheduled processes | OS time zone change notification |
| The system is about to shut down | Save every pane's state | Shutdown notification |
| An OS update is installed | Rerun the toolchain checks | OS version on start |
| The system clock passes a time | Plain schedules: "every weekday at 9:00" | App timer |

### Network

| Trigger | Example process | How it could be detected |
|---|---|---|
| The network comes back after being offline | Push queued commits, sync the team | macOS `NWPathMonitor`, Windows `NotifyIpInterfaceChange`, Linux NetworkManager over D-Bus |
| The network goes away | Switch processes to offline mode | Same |
| I join a specific Wi-Fi network (the office) | Connect to office services, start the office process | SSID: CoreWLAN (location permission needed), WLAN API, NetworkManager |
| I join a metered connection or phone hotspot | Pause large downloads and image pulls | Same |
| A VPN connects or disconnects | Start or stop company only processes | A new network interface |
| A host becomes reachable (staging is up) | Run the smoke tests | Periodic probe |
| A service appears on the local network (mDNS) | A Raspberry Pi, a test device or another Terminal Grid shows up | Bonjour, Windows DNS-SD, Avahi |
| The public IP changes | Update an allowlist | Periodic check |
| Internet speed drops | Pause big syncs | Periodic probe |

### Devices and hardware

| Trigger | Example process | How it could be detected |
|---|---|---|
| A USB device is plugged in | An Arduino or ESP32: flash the firmware; a YubiKey: unlock signing | macOS IOKit, Windows `WM_DEVICECHANGE`, Linux udev |
| A USB drive or SD card is mounted | Copy the camera's photos into the right folder | macOS NSWorkspace mount notification, Windows `DBT_DEVICEARRIVAL`, udisks2 |
| A drive is ejected | Check that the copy finished first | Same |
| A phone is connected | Install the debug build | USB plus `adb` or `xcrun devicectl` |
| An external display is connected | Switch the grid layout to the big screen layout | macOS `CGDisplayRegisterReconfigurationCallback`, Windows `WM_DISPLAYCHANGE`, udev or XRandR |
| A display is disconnected | Back to the laptop layout | Same |
| Bluetooth headphones connect | Start a focus timer | IOBluetooth, Windows Bluetooth APIs, BlueZ |
| The audio output changes | Mute agent sounds on speakers | CoreAudio, WASAPI, PipeWire or PulseAudio |
| A game controller connects | Map buttons to "run process" (a physical button deck) | HID |
| A MIDI device sends a note | Pads on a MIDI controller become process buttons | CoreMIDI, WinMIDI, ALSA |
| A Stream Deck or macro pad button is pressed | Run a process per button | Its plugin SDK, or a hotkey |
| A printer finishes a job | Mark a document as printed in the notes | CUPS on macOS and Linux, the print spooler on Windows |
| A webcam turns on | Treat it as "in a meeting" | See the macOS and Windows section, Linux is partial |
| The CPU gets hot or throttles | Stop parallel agents | Thermal state on macOS, sensors elsewhere |
| A smart card is inserted | Unlock signing processes | PC/SC on all three |

### Input and clipboard

| Trigger | Example process | How it could be detected |
|---|---|---|
| A global hotkey is pressed | "Run my last process", or a different process per hotkey | `tauri-plugin-global-shortcut` (limited on Wayland) |
| A URL is copied | A GitHub issue: start "fix a GitHub issue"; a PR: start a review | macOS `NSPasteboard.changeCount`, Windows `AddClipboardFormatListener`, X11 XFixes (Wayland is limited) |
| A stack trace is copied | Ask an agent to find the bug | Same, plus a pattern check |
| A JSON or SQL blob is copied | Format it or run it against the local database | Same |
| An image is copied | Attach it to an agent prompt | Same |
| Text is selected in Terminal Grid and a button pressed | "Explain", "Fix" or "Make a task" | Already part of session notes |
| A file is dropped on the app window | Run a process on that file | Tauri drag and drop event |
| A file is opened with Terminal Grid ("Open with") | Run the process for that file type | macOS open file event, Windows file association, Linux `.desktop` MimeType |
| The tray or menu bar icon is clicked | A quick menu of processes | Tauri tray |
| Text is typed in a quick launcher | A Spotlight style launcher for processes | A global hotkey plus a small window |

### Time and calendar

| Trigger | Example process | How it could be detected |
|---|---|---|
| A cron style schedule | Nightly dependency audit, weekly cleanup | App timer, plus a catch up run after sleep |
| The first time the computer is used each day | The morning briefing process | Unlock or wake plus a date check |
| The last hour of the working day | Summarise the day, commit WIP | Schedule |
| A calendar event is about to start (from an ICS feed) | Prepare the notes for that meeting | Poll an ICS URL |
| A calendar event ends | Turn the meeting notes into tasks | Same |
| A deadline in a note gets close | Remind me and start the related process | Parse dates in notes |
| N minutes pass in a process | Timeouts and "check again in 10 minutes" loops | Run engine timer |

### Launching from outside

| Trigger | Example process | How it could be detected |
|---|---|---|
| A `terminalgrid://run/<process>` link is clicked | Links in Slack, issues, READMEs and docs start processes | `tauri-plugin-deep-link` |
| A local HTTP endpoint is called | `curl localhost:PORT/run/deploy` from any script or tool | A small local server, token protected |
| The `tg` CLI is run | Scripts, hooks, cron and other tools call into the app | A local socket |
| A browser extension button is pressed | Run a process with the current page URL | Extension plus native messaging, or a deep link |
| An editor extension command runs | "Run process on this file" from VS Code or JetBrains | Extension plus the CLI |
| An MCP tool call from an agent | An agent starts a process ("now run the release process") | Terminal Grid as an MCP server |

### Services the app can poll

These run while the app is open, with the user's own logins, not through the
backend.

| Trigger | Example process | How it could be detected |
|---|---|---|
| A GitHub issue is assigned to me | Start "fix a GitHub issue" with it | `gh api`, polled |
| A review is requested on a PR | Check out the PR in a worktree and start a review | Same |
| CI fails on my branch | Pull the log and ask an agent why | `gh run list` |
| CI passes on my PR | Offer to merge, or move to the next task | Same |
| A PR gets a comment | Answer it, or apply the suggestion | Same |
| A release is published in a dependency | Try the upgrade on a branch | Same |
| A Dependabot or Renovate PR opens | Test it locally | Same |
| A Linear or Jira ticket moves to "In progress" | Create a branch and a worktree for it | Their APIs |
| An error tracker (Sentry and the like) reports a new error | Find the code and propose a fix | Its API |
| An email arrives matching a rule | A bug report email starts triage | IMAP |
| An RSS or Atom feed gets an item | A new advisory for a dependency starts an audit | Poll the feed |
| A Slack message mentions me with a keyword | "deploy" in a channel starts the deploy checklist | Slack API |
| A teammate saves a team process | Show what changed, run its tests | The cloud change feed (`since` and `lastSeq`) |
| A team document changes | Pull it into the local notes | The cloud change feed |
| A template I installed gets a new version | Offer the upgrade | `GET /v1/templates` |

---

## macOS and Windows

These work well on both. Linux support is partial or depends on the desktop
environment.

| Trigger | Example process | macOS | Windows |
|---|---|---|---|
| The frontmost app changes | Switching to the browser on localhost starts the checker | NSWorkspace `didActivateApplication` | `SetWinEventHook` `EVENT_SYSTEM_FOREGROUND` |
| A window with a given title appears | "Zoom Meeting" opens: start meeting mode | Accessibility API | UI Automation |
| The microphone starts being used | Treat it as a call: mute agents, start notes | CoreAudio "running somewhere" property | Microphone usage in the capability access store |
| The camera turns on | Same | CoreMediaIO device property | Webcam usage in the capability access store |
| Teams or Outlook presence changes to "In a call" | Pause noisy processes | Their local logs, or Microsoft Graph | Same |
| An iCloud Drive file syncs | Pick up a file saved on my phone | Watch the iCloud folder | Watch the iCloud for Windows folder |
| My phone shares to the computer | A link sent from the phone starts a process | Universal Clipboard shows up as a clipboard change | Phone Link |
| A Microsoft Office document is saved | Convert it to Markdown into the notes | File watch plus Office's own folders | Same |
| Text is selected in any app and a hotkey pressed | "Send this to an agent" from anywhere | Accessibility API | UI Automation |
| A share menu entry is chosen | "Share to Terminal Grid" from any app | Share extension | Share target (packaged app) |
| A biometric check passes | Gate a step: "Touch to deploy" | Touch ID through LocalAuthentication | Windows Hello |
| The system Focus mode changes | Focus on: pause notifications from processes | Focus filters (App Intents), limited | Focus assist, no public API, hard |
| An Adobe or Figma desktop app exports | A new export folder starts a design sync | File watch | File watch |
| An App Store or Microsoft Store update is installed | Rerun smoke tests for that tool | Store receipts | Store APIs |

## macOS and Linux

Things that come from both being Unix systems.

| Trigger | Example process | How it could be detected |
|---|---|---|
| Terminal Grid receives a Unix signal | `kill -USR1 <pid>` from any script reruns the last process | Signal handler |
| A FIFO (named pipe) is written to | `echo review > ~/.terminal-grid/run` | A FIFO the app reads |
| A cron or `at` job calls the CLI | System cron runs `tg run nightly` | crontab |
| A Homebrew upgrade finishes | Rerun toolchain checks | Watch the Cellar folder, or wrap `brew` |
| A CUPS print job changes state | Same as printing above, with more detail | CUPS notifications |
| A shell rc file changes (`.zshrc`, `.bashrc`) | Reload the shell integration in every pane | File watch |
| A dotfiles repo is pulled | Apply the dotfiles and restart panes | Git watch |
| A Nix or home-manager switch finishes | Rerun toolchain checks | Watch the profile symlink |

## Windows and Linux

Hardware and setups that Macs do not have.

| Trigger | Example process | How it could be detected |
|---|---|---|
| A 2 in 1 switches to tablet mode | Switch to a touch friendly layout | Windows tablet mode setting, libinput `SW_TABLET_MODE` |
| A detachable keyboard is removed | Same | Same |
| A touchscreen or pen is used | Pen gestures run processes | Pointer events by type |
| An NVIDIA GPU job finishes or the GPU goes idle | A training run ended: evaluate and report | `nvidia-smi` or NVML |
| GPU memory runs out | Stop the job and tell me | NVML |
| A WSL distro starts (Windows), or the same distro runs natively (Linux) | One process definition for both setups | `wsl.exe --list --running` on Windows, plain process checks on Linux |

---

## macOS only

| Trigger | Example process | How it could be detected |
|---|---|---|
| A Shortcuts action runs | Siri, the Shortcuts app or an automation starts a process | App Intents |
| A Shortcuts personal automation fires (arrive somewhere, a time, an app opens) | Arrive at the office: start the office process | Shortcuts calls the app's intent |
| A Folder Action fires | The classic macOS folder hook | Folder Actions calling the CLI |
| A Finder Quick Action is chosen | Right click a file, "Run process" | Quick Action or a Finder Sync extension |
| A Services menu item is chosen on selected text | Any app: "Send to Terminal Grid agent" | `NSServices` |
| AppleScript or JXA tells Terminal Grid to run a process | Old style automation from other apps | A scripting dictionary |
| A launchd `WatchPaths` or `StartCalendarInterval` job fires | Runs even when the app is closed, then opens it | A launchd agent |
| An AirDrop file arrives | Lands in Downloads, so the Downloads watch picks it up | File watch |
| A Mail rule runs a script | A support email starts a triage process | Mail rule calling the CLI |
| A Calendar event starts (system calendar) | Meeting prep, without an ICS feed | EventKit (Calendar permission) |
| A Reminder becomes due | Start the process the reminder names | EventKit (Reminders permission) |
| A Time Machine backup finishes | Check it and record it | `tmutil` status, polled |
| An iOS Simulator boots | Install and launch the app build | `xcrun simctl list` |
| An Xcode build finishes or fails | Ask an agent about the errors | Watch DerivedData logs |
| Sidecar or Universal Control connects an iPad | Move a pane to the iPad screen layout | Display reconfiguration |
| Stage Manager or a Space changes | Load the panes for that Space | NSWorkspace active Space notification |
| Apple Music starts a track | Start a focus playlist process: timer and silence | `com.apple.Music.playerInfo` distributed notification |
| The menu bar extra is clicked | Quick list of processes | Tauri tray on macOS |
| The Dock icon is clicked with a modifier | Run the default process | App reopen event |
| A file is dropped on the Dock icon | Run a process on it | Open file event |
| Low Power Mode turns on | Pause heavy processes | `ProcessInfo.isLowPowerModeEnabled` notification |
| The lid closes in clamshell mode | Switch to the external display layout | Display and power notifications |
| A Handoff activity arrives from another Apple device | Continue a process started on another Mac | NSUserActivity |
| Keychain is unlocked | Allow signing steps | Keychain status |
| Spotlight shows the processes | Search "fix issue" in Spotlight to run it | Core Spotlight items plus App Intents |
| The screenshot location changes | Keep the screenshot trigger working | `com.apple.screencapture` `location` default |

## Windows only

| Trigger | Example process | How it could be detected |
|---|---|---|
| A Windows Event Log entry matches | An app crash (event 1000) starts "collect the dump and open an issue" | `EvtSubscribe` |
| Windows Update installs an update | Rerun toolchain checks | Event Log, setup channel |
| Microsoft Defender flags a file | Stop processes using that folder, tell me | Event Log, Defender channel |
| A Task Scheduler trigger fires (on event, on unlock, on idle) | Runs even when the app is closed, then opens it | A scheduled task calling the CLI |
| A WMI event fires | Process start traces, USB, volume changes, all in one place | WMI event queries |
| A registry key changes | A tool's setting changes, so restart the panes using it | `RegNotifyChangeKeyValue` |
| A File Explorer context menu entry is chosen | Right click a folder, "Open in Terminal Grid and run…" | Shell verb in the registry, or `IExplorerCommand` |
| A Jump List task is clicked | Right click the taskbar icon, pick a process | Jump List tasks |
| A toast notification button is pressed | "Run tests?" with Yes and No | Toast activation |
| A Win+Shift+S snip is taken | An image lands on the clipboard: attach it to an agent | Clipboard listener plus the Screenshots folder |
| The virtual desktop changes | Load the panes for that desktop | `IVirtualDesktopManager` is limited, a registry watch works |
| A WSL distro starts or stops | Start the panes that live inside it | `wsl.exe --list --running`, polled |
| A Hyper-V VM or Windows Sandbox starts | Run the tests that target it | Hyper-V WMI |
| A Dev Drive is mounted | Point build caches at it | Volume mount events |
| A Visual Studio build ends | Ask an agent about the errors | Build log, or the VS extension SDK |
| An Outlook email arrives (desktop Outlook) | Mail rule style triggers without IMAP | Outlook COM |
| PowerToys Run or Command Palette runs a command | Launcher integration for processes | Their plugin APIs |
| The app is started by a Windows Terminal profile | "Open Terminal Grid here" from Windows Terminal | Command line arguments |
| A USB device of a given vendor arrives | Same as the general USB row, via WMI rules | `Win32_DeviceChangeEvent` |
| The user signs in with Windows Hello | Start the morning process | Session change after unlock |

## Linux only

| Trigger | Example process | How it could be detected |
|---|---|---|
| Any D-Bus signal matches a rule | A general escape hatch: anything on the bus starts a process | D-Bus match rules |
| A systemd unit fails | My local service crashed: show its journal in a pane | `org.freedesktop.systemd1` signals |
| A systemd user timer fires | Runs even when the app is closed | A timer unit calling the CLI |
| A journald entry matches | Kernel or service errors start a process | `journalctl -f` with filters |
| A udev rule fires | Very precise device triggers (one board by serial) | udev rule calling the CLI |
| A desktop notification from another app is shown | A CI tool's notification starts a process | Watch `org.freedesktop.Notifications` on the session bus |
| An MPRIS media player starts or stops | Focus mode with music | MPRIS over D-Bus |
| KDE Connect forwards a phone notification | A 2FA or alert on the phone starts a process | KDE Connect over D-Bus |
| An apt, dnf or pacman transaction finishes | Rerun toolchain checks | Package manager hooks, or its log |
| A Flatpak or Snap updates | Same | Their CLIs, polled |
| The workspace changes | Load the panes for that workspace | X11 `_NET_CURRENT_DESKTOP`, sway, Hyprland or KDE IPC |
| A window manager event fires (sway, Hyprland, i3) | Tiling events become triggers | WM IPC sockets |
| A file manager action is chosen | Right click, "Run process" | Nautilus scripts, Dolphin service menus, Thunar custom actions |
| A file in `/etc` changes | Config drift: show the diff | inotify (permissions allowing) |
| A container in Podman starts | Same as the Docker rows, rootless | `podman events` |
| GNOME or KDE night light turns on | Evening wind down process | The desktop's D-Bus interface |
| Power profile changes (power saver, performance) | Match the agent parallelism to it | `power-profiles-daemon` over D-Bus |

---

## Permissions and caveats

- **macOS** asks the user before an app may use Accessibility (window titles, selected
  text), Screen Recording (window contents), Calendar and Reminders, Location (the
  Wi-Fi network name), and before it reads some folders (Desktop, Documents,
  Downloads). Each trigger that needs one should ask the first time it is used.
- **Windows** gates far less, but packaged apps (MSIX, Microsoft Store) need
  capabilities declared for some of these, such as the share target or toast actions.
- **Linux on Wayland** blocks global hotkeys, other apps' window titles and the
  clipboard in the background, unless the desktop environment offers a portal for it.
  X11 allows most of them.
- **Privacy.** Clipboard, window title and microphone or camera triggers read
  sensitive things. They should be off by default, need a clear opt in, and process
  definitions shared with a team should never carry what they read.
- **Polling cost.** Process lists, ports and service checks should share one poller
  with sensible intervals, as `AgentWatcher` does, not one loop per trigger.
- **When the app is closed.** Most triggers only work while Terminal Grid runs. The
  OS schedulers (launchd, Task Scheduler, systemd timers) and the CLI are the way to
  catch things while it is closed.
- **Shared definitions.** A trigger in a team process should use names, not paths
  that exist on one machine (`${repo.root}`, `${downloads}`), the same rule as for
  steps.

---

# Part 2: Design

## Design: one crate per integration

### Goals

- **One integration, one crate.** Each source of triggers is a Rust crate under
  `src-tauri/crates/` with its own tests and README. It depends on the contract
  crate only and knows nothing about Terminal Grid, so it can be built, tested and
  reviewed on its own.
- **The app composes the crates.** Adding a source is adding a crate and one line
  in the app. Removing one is a cargo feature.
- **Every crate compiles on all three platforms.** A source that cannot run where
  it is says so at runtime (`Unsupported` or `NeedsPermission`), never by failing
  to build. Platform code sits behind `cfg`.
- **Small models can build a crate.** The contract is fixed, the shape of a crate
  is always the same, and most of the logic is pure and unit tested. A card can
  say "implement `Source` for X" and mean something exact.
- **Definitions stay shareable.** A trigger in a team process names what to watch
  with variables (`${repo.root}`), never a path that exists on one machine, the
  same rule as for steps. Secrets never enter a definition or an event payload.

### Layout

```
src-tauri/
  Cargo.toml                  the app, and now the workspace root
  src/triggers/
    mod.rs                    the app side: TriggerService, commands, events
    internal.rs               triggers from inside Terminal Grid (epic 002)
    variables.rs              ${repo.root} and friends
  crates/
    trigger-core/             the contract and the test kit
    trigger-hub/              registry, rules, cool down, catalogue
    trigger-fs/               files and folders (the reference source)
    trigger-git/              commits, branches, pulls, conflicts
    trigger-process/          apps starting and stopping, crashes, ports
    trigger-system/           wake, sleep, lock, idle, battery, displays, theme
    trigger-network/          online, Wi-Fi, VPN, reachability, mDNS
    trigger-clock/            schedules and calendar feeds
    trigger-input/            clipboard, global hotkeys, drops, open with
    trigger-devices/          USB, drives, Bluetooth, audio, MIDI, HID
    trigger-docker/           Docker and Podman events
    trigger-github/           issues, reviews, CI, releases, polled with gh
    trigger-launch/           deep links, the tg CLI, a local endpoint, hooks
    trigger-macos/            Shortcuts, Folder Actions, Services, Calendar
    trigger-windows/          Event Log, Task Scheduler, WMI, Explorer menu
    trigger-linux/            D-Bus, systemd, journald, udev
```

The workspace is declared in `src-tauri/Cargo.toml` (`[workspace] members =
["crates/*"]`), with shared versions in `[workspace.dependencies]` so every crate
uses the same `serde`, `notify` and `sysinfo`. CI runs `cargo test --workspace`
on all three platforms.

### The contract: `trigger-core`

Everything a source needs, and nothing about the app. The types below are the
contract; epic 001 writes them out in full.

```rust
/// A kind id is "<source>.<kind>", for example "fs.file-saved". Stable:
/// renaming one breaks shared definitions.
pub struct KindInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub platforms: Platforms,            // MACOS | WINDOWS | LINUX, a bit set
    pub permission: Option<Permission>,  // what the OS will ask the user for
    pub when: &'static [FieldInfo],      // the fields a rule may set
    pub payload: &'static [FieldInfo],   // the fields an event carries
}

pub struct FieldInfo { pub name: &'static str, pub doc: &'static str, pub required: bool }

pub enum Permission {
    Accessibility, ScreenRecording, Location, Calendar, Reminders,
    Folder(&'static str), Microphone, Camera, Notifications,
}

pub enum Availability {
    Available,
    Unsupported { reason: String },                        // wrong OS, missing tool
    NeedsPermission { permission: Permission, how: String },
}

/// One trigger of one armed process, variables already filled in by the app.
pub struct Rule {
    pub id: RuleId,                                  // "<process id>/<trigger id>"
    pub kind: String,                                // "fs.file-saved"
    pub when: serde_json::Map<String, serde_json::Value>,
}

pub struct Event {
    pub rule: RuleId,
    pub kind: String,
    pub at: std::time::SystemTime,
    pub payload: serde_json::Map<String, serde_json::Value>,
}

pub type Sink = std::sync::mpsc::Sender<Event>;

pub trait Source: Send + Sync + 'static {
    fn id(&self) -> &'static str;                    // "fs"
    fn kinds(&self) -> &'static [KindInfo];
    fn availability(&self) -> Availability;
    /// Checks the `when` fields of a rule. Run before start, and when a user
    /// edits a trigger, so mistakes show up in the editor and not at runtime.
    fn check(&self, rule: &Rule) -> Result<(), RuleError>;
    /// Starts watching. Events go to the sink until the handle is dropped.
    fn start(&self, rules: Vec<Rule>, sink: Sink) -> Result<Handle, SourceError>;
}

/// Dropping it stops the watcher.
pub struct Handle(Box<dyn FnOnce() + Send>);
```

`trigger_core::testkit` ships with the crate: `RecordingSink` (collects events,
`wait_for(kind, timeout)`), `rule(kind, json!({...}))`, a `Clock` trait with a
fixed clock, and `FakeSource` for the hub's tests.

Rules every source follows:

1. A payload is JSON scalars and short strings: paths, names, ids, numbers. Never
   file contents, never more than 4 KiB, never anything that looks like a secret.
2. `start` returns at once. The source spawns its own thread or uses the OS
   callback. Stop is `Drop`, and it must not block for more than a second.
3. No replay. A source fires for what happens after `start`, not for state it
   found when it looked (a file that already existed, a process already running).
   Schedules catch up on a missed time only when their rule says so.
4. The source debounces where the OS is noisy (a save is three events on Windows).
   The hub adds a cool down per rule on top.
5. The source parses `when` into its own typed struct with serde, and `check`
   rejects unknown fields, so a typo in a definition is an error in the editor.
6. Platform code is behind `#[cfg(target_os = "...")]`; the rest of the crate is
   the same on every OS. Platform only dependencies go under
   `[target.'cfg(target_os = "...")'.dependencies]`.
7. One crate, one source, one id prefix. `trigger-process` fires `process.*`,
   including `process.port-opened`.

### Anatomy of a source crate

```
crates/trigger-fs/
  Cargo.toml          trigger-core = { path = "../trigger-core" }, notify (workspace)
  README.md           the kinds table, platforms, permissions, payload fields,
                      how to run the live tests, what is known not to work
  src/
    lib.rs            pub struct FsSource; impl Source; pub static KINDS
    rule.rs           the typed `when` struct, parsing, check (pure)
    matcher.rs        pure: does an OS event match a rule, debounce state machine
    watch.rs          the OS layer behind a small trait (trait Watch)
    windows.rs        only where one platform needs its own code
  tests/
    live.rs           #[ignore] tests against the real OS
```

Three levels of tests, so a crate is trusted before it ever touches the OS:

| Level | What | Runs where |
|---|---|---|
| Pure | rule parsing, matching, state machines (`step` functions, like `agent::step` today) | every `cargo test`, all three platforms in CI |
| Fake OS | the OS layer is a trait; tests drive a fake and read the sink | every `cargo test` |
| Live | `#[ignore]` tests against the real OS: `cargo test -p trigger-fs -- --ignored` | a developer's machine; CI too for the crates where the runner allows it (fs, git, clock, process) |

A crate's README is part of its definition of done: the kinds table in it is
generated from `KINDS` by `cargo run -p trigger-hub --example catalogue`, and a
test fails when the README is stale.

### The hub: `trigger-hub`

The one place that knows every source. It is still app agnostic.

- `Hub::new(sources: Vec<Box<dyn Source>>)`.
- `catalogue()`: every source with its kinds and current availability. This is
  what the trigger picker in the UI shows, and what regenerates the README tables.
- `apply(rules: Vec<Rule>)`: groups rules by source, runs `check` on each, starts
  the sources that have rules and restarts the ones whose rules changed, drops
  the rest. Idempotent, so the app can call it whenever anything changes.
- `publish(event)`: for triggers raised by the app itself (the "Inside Terminal
  Grid" section of the catalogue). Those need no crate.
- `subscribe() -> Receiver<Fired>`: an event plus the process it belongs to, after
  a cool down per rule (default two seconds) so a burst is one trigger.
- Tests use `FakeSource` from the test kit: rules reach the right source, a
  changed rule restarts it, a source error is reported and does not stop the
  others.

### In the app

`src-tauri/src/triggers/mod.rs` owns a `TriggerService` in `AppState`:

- `sources()` lists every crate, each behind a cargo feature that is on by
  default, so a build can leave one out.
- Rules come from the **armed** processes on this machine. Definitions live in
  `<app config dir>/processes/<id>.json` (the same `ProcessDefinitionV1` the cloud
  stores; the team sync epic later writes into the same folder). Which processes
  are armed is local, in `config.json`, off by default and never synced.
- `variables.rs` fills in `${repo.root}` (every repo with a pane open, one rule
  each), `${home}`, `${downloads}`, `${notes.root}` and `${screenshots}`. A rule
  whose variable has no value here is skipped and the UI says why.
- Commands: `triggers_catalogue`, `triggers_check` (a rule, for the editor),
  `triggers_status` (armed rules, availability, last fired). Event:
  `trigger://fired`.
- Permissions are asked for when a rule that needs one is armed, never at start.
- Until the run engine exists (its own epic), a fired trigger shows a toast and is
  logged. The run engine then subscribes to the same stream.

### In a process definition

```json
{
  "schemaVersion": 1,
  "name": "Keep the docs tidy",
  "triggers": [
    { "id": "docs-saved", "kind": "fs.file-saved",
      "when": { "path": "${repo.root}/docs", "glob": "**/*.md" } }
  ],
  "lanes": [ ... ]
}
```

`triggers` is an optional field, so `schemaVersion` stays 1 and older apps ignore
it. Steps can use the payload: `${trigger.path}`, `${trigger.kind}`. The cloud
repo gets a `ProcessTrigger` schema in `ProcessDefinitionV1` as a follow up; the
server stays indifferent to the field either way.

### Which crate covers what

| Catalogue section | Crate | Source id |
|---|---|---|
| Inside Terminal Grid | the app (`src/triggers/internal.rs`) | `tg` |
| Files and folders | `trigger-fs` | `fs` |
| Git and code | `trigger-git` | `git` |
| Developer tools: ports, editors, agents outside the app | `trigger-process` | `process` |
| Developer tools: containers | `trigger-docker` | `docker` |
| Apps and processes | `trigger-process` | `process` |
| System state | `trigger-system` | `system` |
| Network | `trigger-network` | `network` |
| Devices and hardware | `trigger-devices` | `devices` |
| Input and clipboard | `trigger-input` | `input` |
| Time and calendar | `trigger-clock` | `clock` |
| Launching from outside | `trigger-launch` | `launch` |
| Services the app can poll | `trigger-github` (GitHub, CI, Dependabot); the rest later | `github` |
| macOS and Windows | split between `trigger-process` (frontmost app, window titles) and `trigger-devices` (mic, camera) | |
| macOS and Linux, Windows and Linux | folded into `trigger-launch` (signals, FIFOs) and `trigger-system` (tablet mode, GPU) | |
| macOS only | `trigger-macos` | `macos` |
| Windows only | `trigger-windows` | `windows` |
| Linux only | `trigger-linux` | `linux` |

Not covered yet, by choice: Slack, Linear, Jira, Sentry, email and RSS. Each is a
small polling crate on the same pattern as `trigger-github`, worth adding when a
process needs it.

## Building it with the planning pipeline

The pipeline from the cloud repo now lives here too: `plans/` and `.claude/`.
`plans/README.md` explains the three tiers (Opus architects, Sonnet researches
and writes cards, Haiku builds, Sonnet checks). The same rules apply: cards name
exact files, acceptance is commands, a stuck builder stops.

The work is cut so that crates can be built in parallel:

| Epic | Crate | Depends on |
|---|---|---|
| `001-trigger-core` | workspace, `trigger-core`, `trigger-hub`, `trigger-fs`, the app wiring and the Settings switch | |
| `002-trigger-internal` | triggers from inside the app, published into the hub | 001 |
| `003-trigger-git` | `trigger-git` | 001 |
| `004-trigger-process` | `trigger-process` | 001 |
| `005-trigger-system` | `trigger-system` | 001 |
| `006-trigger-network` | `trigger-network` | 001 |
| `007-trigger-clock` | `trigger-clock` | 001 |
| `008-trigger-input` | `trigger-input` | 001 |
| `009-trigger-devices` | `trigger-devices` | 001 |
| `010-trigger-docker` | `trigger-docker` | 001 |
| `011-trigger-github` | `trigger-github` | 001 |
| `012-trigger-launch` | `trigger-launch` | 001 |
| `013-trigger-macos` | `trigger-macos` | 001 |
| `014-trigger-windows` | `trigger-windows` | 001 |
| `015-trigger-linux` | `trigger-linux` | 001 |

Epic 001 is the only one on the critical path. Every other epic depends on 001
alone, so once 001 is merged they can be planned in any order and built at the
same time, each on its own branch (`epic/<id>`) in its own git worktree, and
merged in any order. The process editor, the run engine and the trigger picker
are later epics that build on the same stream of events.

Every source epic has the same shape, which is what makes the cards easy to write
and build:

1. **Decisions** name the OS mechanism per platform (the research notes supply
   the facts: crate versions, API behaviour, what CI runners allow).
2. **Shape** writes out the `KINDS` table with every id, the typed `when` struct
   and the payload fields. Those ids are the contract with definitions.
3. **Slices**: A the pure core (rule, matcher, kinds) with unit tests; B the OS
   layer, one card per platform; C live tests, README, the cargo feature and the
   one line that registers the source in the app; D the repo map update.
4. **Done** means: `cargo test --workspace` and `cargo clippy --workspace
   --all-targets -- -D warnings` pass on all three platforms in CI, the live
   tests pass on at least one real machine per supported platform (the Log says
   which), and the README's kinds table matches `KINDS`.

# Plans

How work gets planned and built in this repo. The idea: spend the expensive
model where judgement matters, and give cheap, fast models work so well
specified that they cannot go far wrong. This is the same pipeline as the
cloud repo (`Terminal-Grid-Cloud/plans/`), with this repo's context files.

## The three tiers

| Tier | Model | Job | Writes |
|---|---|---|---|
| Architect | Opus (`claude --model opus`) | Decides the shape of an epic, names what it does not know, reviews the final cards | `epic.md` |
| Gap fillers | Sonnet (`gap-researcher`, `task-writer`) | Answers one research question each; turns one slice of an epic into task cards | `research/*.md`, `tasks/*.md` |
| Builders | Haiku (`task-builder`), checked by Sonnet (`task-checker`) | Builds exactly one task card, nothing else | code |

Opus never writes the detail itself and Haiku never makes a design decision.
Sonnet sits in between: it can read code and docs carefully, but it works
inside decisions Opus already made.

## The flow

```
/plan-epic 001-trigger-core             (run in an Opus session)
  1. Architect reads plans/_context/*, the codebase and the epic brief
  2. Architect writes epic.md: goal, decisions, slices, open questions
  3. gap-researcher (Sonnet) x N, one per open question, in parallel
  4. Architect folds the answers into Decisions, re-slices if needed
  5. task-writer (Sonnet) x N, one per slice, in parallel
  6. node plans/_tools/lint-cards.mjs <epic>     (structure check)
  7. Architect reviews every card, fixes or sends back
  8. Optional: /dry-run-epic <id> builds everything in a throwaway
     worktree and reports which cards confused Haiku

/build-epic 001-trigger-core            (any session; Sonnet is plenty)
  on branch epic/<id>, one card at a time in dependency order:
     task-builder (Haiku) builds it
     task-checker (Sonnet) runs the acceptance checks and the scope check
     pass  -> status: done, commit
     fail  -> one more Haiku attempt with the checker's notes
     fail  -> one Sonnet attempt (escalation)
     fail  -> status: blocked, reason in the card's Log; Opus looks later
```

`/build-task <card path>` runs that loop for a single card.

## Building epics in parallel

The trigger epics (002 and up) depend on 001 only, so once 001 is merged they
can be built at the same time. Give each its own worktree so builds do not
step on each other:

```
git worktree add ../Terminal-Grid-003 -b epic/003-trigger-git main
cd ../Terminal-Grid-003 && npm ci && claude --model sonnet   then   /build-epic 003-trigger-git
```

Each epic touches its own crate under `src-tauri/crates/`, plus a few shared
lines (the workspace member list, the feature in the app's `Cargo.toml`, the
one line that registers the source). Those are the only places merges meet.

## Folder layout

```
plans/
  README.md                 this file
  _context/                 shared context every tier reads
    repo-map.md             where things live and which command does what
    conventions.md          code, test, commit and writing rules
    architecture.md         the app's big picture and the triggers design (Opus owned)
  _templates/               epic.md, research.md, task.md
  _tools/lint-cards.mjs     structural checks for task cards
  _tools/dashboard.mjs      live progress page (see Watching progress)
  _runs/                    local activity log for the dashboard, git ignored
  NNN-slug/                 one folder per epic, numbered in build order
    epic.md
    research/RNN-slug.md
    tasks/TNN-slug.md
```

The design the epics implement is `docs/process-triggers.md`, Part 2.

## Rules that keep the cards foolproof

1. **Cards are written one epic ahead of the build, not further.** A card
   points at exact files and functions; if those do not exist yet the card is
   guessing. Epics further out stop at research.
2. **One card, one reviewable change.** Size S is under ~150 changed lines, M
   under ~400. Anything bigger gets split.
3. **Every file a card may touch is listed.** The checker fails a build that
   touches anything else (lock files and generated files the card names are
   allowed).
4. **Acceptance is commands, not prose.** Each check is a command and what it
   must print or return. If something cannot be checked by a command, it says
   exactly what to look at.
5. **Code is given where it is not obvious.** Types, signatures, config blocks,
   `cfg` attributes and anything with a non obvious shape are written out in
   the card. The builder fills in bodies, not designs.
6. **Stop beats improvise.** Every card has an "If stuck" section. A builder
   that hits something the card did not foresee sets `status: blocked` and
   writes what it saw, instead of inventing a fix.
7. **Context is pointed to, not pasted.** A card lists at most about six
   files to read, with line ranges where it helps. `_context/` covers the rest.
8. **Platform work is one card per platform.** A card that says "on macOS do
   X, on Windows do Y" is two cards. The builder may be on either.

## Card status

`todo` → `in-progress` → `done`, or `blocked`. The status lives in the card's
front matter and is the only state the build loop needs. Every attempt adds a
line to the card's Log section.

## Running it by hand

```
claude --model opus      then   /plan-epic 001-trigger-core
claude --model sonnet    then   /build-epic 001-trigger-core
claude --model sonnet    then   /build-task plans/001-trigger-core/tasks/T03-trigger-core-types.md
node plans/_tools/lint-cards.mjs 001-trigger-core
node plans/_tools/status.mjs            # table of every card and its status
```

## Watching progress

```
node plans/_tools/dashboard.mjs --open      # http://localhost:4317
```

A live page with every epic's progress, a graph of each epic's cards
coloured by status, what is running right now with a timer, the activity
feed, and recent commits. It reads `plans/` and the git state of this repo,
and updates by itself when they change.

The skills write each step (builder started, checker verdict, retry,
escalation, commit, blocked) to `plans/_runs/activity.jsonl` with
`plans/_tools/log-event.mjs`. That file is git ignored: it is a local record
of runs on this machine, not part of the plan. Clicking a card shows its
goal, the files it may touch and its Log.

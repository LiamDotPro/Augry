---
name: build-task
description: Build one task card with a Haiku builder, verify it with a Sonnet checker, retry once, escalate to Sonnet once, otherwise mark it blocked.
argument-hint: <card path, e.g. plans/001-trigger-core/tasks/T03-trigger-core-types.md>
model: sonnet
disable-model-invocation: true
---

# Build one card

Card: $ARGUMENTS

You orchestrate one card. You do not write code yourself.

Record every step for the progress dashboard
(`node plans/_tools/dashboard.mjs`) with
`node plans/_tools/log-event.mjs <event> <epic> <card> [--model m] [--result r] [--attempt n] [--note text]`,
run from this repo. The events are named in each step below. Logging never
blocks the build: if the command fails, carry on.

1. Read the card. If its `status` is `done`, say so and stop. If any card in
   `depends_on` is not `done`, say which and stop.
2. **Attempt 1.** Log `build-start` (with `--model` and `--attempt 1`). Launch a `task-builder` agent, passing the card's `model:`
   value (default `haiku`) as the model override, with the prompt:
   "Build the task card at <card path>. Follow .claude/agents/task-builder.md."
3. Log `build-end` with `--result pass` or `--result blocked`. If the builder reports BLOCKED, go to step 7.
4. Log `check-start --model sonnet`, then launch a `task-checker` agent:
   "Check the task card at <card path>. Follow .claude/agents/task-checker.md."
5. Log `check-end --result pass` or `--result fail`. **On PASS**: set the card's `status: done`, add a Log line
   `- YYYY-MM-DD checker: pass`. Done.
6. **On FAIL**, in order:
   - If the checker reports a CARD PROBLEM, do not retry: go to step 7 with
     that problem as the reason.
   - Attempt 2 (log `retry --attempt 2`): discard the builder's changes but keep the card file:
     copy the card to a temp file, run `git reset --hard HEAD` and
     `git clean -fd -- <each path under the card's files:>`, copy the card
     back. Never use bare `git stash`; other sessions may share the stack. (Never use bare `git stash`; other sessions may share the stack.)
     Then launch `task-builder` again with the
     checker's NOTES FOR RETRY appended to the prompt. Check again.
   - Attempt 3 (log `escalate --model sonnet --attempt 3`): discard again the same way and launch `task-builder` with
     model override `sonnet` and both sets of notes. Check again.
   - Still failing: step 7.
7. **Blocked.** Log `blocked --note <reason in a few words>`. Set the card's `status: blocked` and add a Log line with the
   reason (the builder's or checker's exact words, trimmed). Leave the
   working tree as it is for the caller to deal with.
8. Report PASS or BLOCKED with the reason, attempts used, and the model that
   succeeded.

---
name: build-epic
description: Build every ready card of an epic in dependency order with Haiku builders and Sonnet checkers, committing after each passing card.
argument-hint: <epic id, e.g. 001-trigger-core>
model: sonnet
disable-model-invocation: true
---

# Build an epic

Epic: $ARGUMENTS

You orchestrate. You do not write code yourself, and you do not edit cards
apart from their `status` and Log.

## Setup

1. `node plans/_tools/status.mjs $ARGUMENTS` to see every card.
2. `node plans/_tools/lint-cards.mjs $ARGUMENTS`. If it reports errors, stop
   and report them; the epic is not ready.
3. Make sure the working tree is clean (`git status --porcelain` empty,
   ignoring `plans/_runs/`). If not, stop and say so.
4. If not already on it, create and switch to branch `epic/$ARGUMENTS`.
5. Set the epic's `status: building` and log
   `node plans/_tools/log-event.mjs epic-start $ARGUMENTS`.

## Loop

Repeat until no card is `todo` with all `depends_on` done:

1. Pick the lowest numbered card that is `todo` and whose `depends_on` are
   all `done` (ids like `001-api-foundation/T04` refer to other epics).
2. Run the single card loop from `.claude/skills/build-task/SKILL.md` on it.
3. On PASS: log `commit <epic> <card>` and commit everything, card file
   included, with message `<epic> <card id>: <card title>`. Continue.
4. On BLOCKED: copy the card file to a temp file, discard every other
   change (`git reset --hard HEAD`, then `git clean -fd -- <each path under
   the card's files:>`), copy the card back and commit it alone with message
   `<epic> <card id>: blocked`. Skip every
   card that depends on it. Continue with the rest.

Cards run one at a time in this checkout so each sees the last one's work.

## Finish

Run `node plans/_tools/status.mjs $ARGUMENTS`. If every card is done, set the
epic's `status: done`, log `epic-end $ARGUMENTS`, and commit. Report: cards done, cards blocked with the
one line reason from their Log, escalations to Sonnet, and the branch name.
Do not merge or push.

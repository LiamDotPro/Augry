---
name: plan-epic
description: Plan an epic end to end. Opus architects, Sonnet researchers fill the gaps, Sonnet card writers produce task cards that Haiku can build. Run in an Opus session.
argument-hint: <epic id, e.g. 003-trigger-git> or "new: <idea>"
model: opus
disable-model-invocation: true
---

# Plan an epic

Epic: $ARGUMENTS

You are the architect. Your judgement goes into decisions and review; the
detail work goes to cheaper agents. Read `plans/README.md` first if you have
not this session; its rules are binding.

Log each phase for the progress dashboard with
`node plans/_tools/log-event.mjs <event> <epic> [id] [--model m] [--note text]`:
`plan-start`/`plan-end` around step 1, `research-start`/`research-end` with
the question id around each researcher, `carding-start`/`carding-end` with
the slice letter around each card writer, `review-start`/`review-end`
around step 5. Logging never blocks planning.

## 0. Orient

- Read `plans/_context/` (all files), `CLAUDE.md`, and the epic folder if it
  exists. If the argument starts with `new:`, pick the next free number,
  create `plans/NNN-slug/` and copy `plans/_templates/epic.md` into it.
- Check `depends_on`: epics this one builds on should be `done`, or at least
  carded, otherwise card writers will be guessing at code that does not exist.
  If they are not, stop after step 3 (research) and say so. See README rule 1.
- Read the code the epic will touch. Enough to make real decisions, not all
  of it.

## 1. Architect (you)

Fill in `epic.md`: Goal, Non goals, Decisions, Shape, Slices, Open questions,
Risks. Set `status: researching`.

- Decisions are final and numbered. Make them. Leave a question open only if
  the answer is a fact you cannot get from the repo (versions, library
  behaviour, platform limits, current docs).
- Shape must be concrete enough that two card writers on different slices
  produce code that fits: write shared types, interfaces, routes, tables,
  file paths and module names out in full.
- Slices are in build order, each 2 to 6 cards of work.
- Open questions are answerable with a fact or a short comparison.

## 2. Research (Sonnet, parallel)

For each open question, launch a `gap-researcher` agent, all in one message
so they run in parallel. Prompt each with: epic id, question id, the question
verbatim, why it matters, and the output path
`plans/<epic>/research/RNN-slug.md`.

## 3. Fold in

Read every research note. For each:

- Copy the short answer into the Open questions table.
- If it changes anything, update Decisions or Shape. If a note says
  `contradicts-decision`, decide explicitly and record the reason.
- If a note is `partial` and the gap matters, run one more researcher with a
  narrower question, or make the call yourself and record it.

Set `status: carding`.

## 4. Card writing (Sonnet, parallel)

Give each slice a card id range (A: T01 to T09, B: T10 to T19, and so on so
ids never collide). Launch one `task-writer` per slice in one message.
Prompt each with: epic id, slice letter and description from the Slices
table, its id range, and the ids and titles of earlier slices' cards it may
depend on (from the Slices table; the writer will read those cards if they
already exist).

If slices depend on each other's file names heavily, run them in order
instead, earliest slice first.

## 5. Review (you)

1. `node plans/_tools/lint-cards.mjs <epic>` and fix every error.
2. Read every card as if you were Haiku seeing this repo for the first time:
   - Is any step a decision in disguise ("choose", "set up appropriately",
     "handle errors")? Write the decision into the step.
   - Is every path real, or created by an earlier card in `depends_on`?
   - Do the cards together cover the epic's Goal and nothing more?
   - Do acceptance checks actually prove the goal, or just that it compiles?
3. Fix small problems yourself. Send a slice back to a `task-writer` with
   specific notes when it needs rework.
4. Answer any decisions the writers asked for, update `epic.md`, and re-card
   affected slices.

Set `status: ready`.

## 6. Report

Tell the user: cards per slice, the critical path, any card marked
`model: sonnet` and why, open risks, and suggest `/dry-run-epic <epic>` for
epics where a failed build would be costly.

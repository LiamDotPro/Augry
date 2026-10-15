---
name: dry-run-epic
description: Prove an epic's cards are buildable by running the whole build in a throwaway git worktree, then report which cards confused the builders. Changes nothing on your branch except the report.
argument-hint: <epic id>
model: sonnet
disable-model-invocation: true
---

# Dry run an epic

Epic: $ARGUMENTS

1. Create a throwaway worktree from the current HEAD:
   `git worktree add -b dryrun/$ARGUMENTS ../$(basename "$PWD")-dryrun-$ARGUMENTS HEAD`
2. In that worktree, follow `.claude/skills/build-epic/SKILL.md` for
   $ARGUMENTS, with two changes: never escalate to Sonnet (a card only Sonnet
   can build is a finding), and keep going past blocked cards where the
   dependency graph allows.
3. Collect, for every card: attempts used, final status, and every
   CARD PROBLEM and NOTES FOR RETRY the checker gave.
4. Write `plans/$ARGUMENTS/dry-run.md` **in the original checkout**: a table
   of cards with result and attempts, then for each card that needed a retry
   or blocked, the exact fix the card needs.
5. Remove the worktree (`git worktree remove --force <path>`) and delete the
   `dryrun/$ARGUMENTS` branch, unless the user asked to keep it.
6. Report the table. Card fixes are the architect's job: suggest running
   `claude --model opus` and asking it to apply `dry-run.md`.

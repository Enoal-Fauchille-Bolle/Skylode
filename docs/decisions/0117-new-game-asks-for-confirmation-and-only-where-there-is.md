# 0117 — New game asks for confirmation, and only where there is a run to lose

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**`New game` asks for confirmation, and only where there is a run to lose**

## Why

A departure from [UI.md](../UI.md#61-splash) §6.1's frame, which draws no confirmation.
`New game` sits one arrow key from `Continue`, and the new run's first write — ten
seconds later — rotates the old save into the backup slot and takes the good backup with
it, so the only rescue is a twenty-second window. The box is skipped on a fresh install
and after a recovery, where it would be a question with one answer; and its caret opens
on **`No`**, since a reflexive `Enter` must not land on the destructive side. The same
honesty applies to the recovery frames' own `Start a new game` row, whose old *"the
current save is kept"* was true for about ten seconds and now reads *"the backup goes
with it"*.

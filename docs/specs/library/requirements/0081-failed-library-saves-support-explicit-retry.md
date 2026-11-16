---
type: REQ
profile-version: 1
id: "REV-REQ-0081"
title: "Failed Library saves support explicit Retry"
---

# Failed Library saves support explicit Retry

## Statement

WHEN a Library metadata or reading save fails temporarily or has an uncertain outcome, the Library MUST offer explicit
Retry of the retained attempt under its original editing baseline. It MUST preserve submitted input through refusal or
uncertainty and use authoritative recovery to recognise a committed save. Ordinary recovery MUST NOT replay a save.

## Rationale

A reader must be able to recover a failed save without reconstructing their input or overwriting changes they never saw.
An uncertain acknowledgement does not establish whether the write committed. Keeping the original attempt and
recognising authoritative success avoids both lost input and duplicate Undo entries.

## Acceptance criteria

- E1: Temporary or uncertain failures offer "Not saved. Retry" and resend the retained attempt on activation, showing
  "Retrying…" while pending. Metadata conflicts resolve the original request first. A conflict not confirmed as saved
  uses the existing matched "Not saved. Reload" or new-edit path. Validation failures retain input and field feedback
  for correction in the editor. Permission and locked-field refusals keep specific feedback without an unchanged resend.
  Batch actions classify each target independently, offer Retry only for temporary or uncertain failures, and never
  resend successful or conflicted targets. No bulk Reload control is offered.
- E2: Explicit Retry sends the original submitted payload and starting baseline for the failed metadata or reading edit.
  Background reads, live record merges and precondition refusals do not advance that baseline or reconstruct the input.
- E3: An unchanged metadata attempt reuses its request UUID, patch and baseline. A stale repeat resolves that UUID
  before conflict feedback. Authoritative saved resolution adopts its matching acknowledgement; unknown resolution
  remains uncertain. Reading recovery uses a matched read. Ordinary recovery sends no write, and a request UUID alone
  does not prove idempotency or success.
- E4: Successful Retry publishes its matched acknowledgement and eligible exact Undo once. Refused or unresolved Retry
  retains submitted input and conflict or recovery feedback. Reload sends no write and adopts fresh selected values with
  their matching baseline while keeping the failed submitted value available. Only an explicit matched reset or a new
  editing decision can adopt that new pair. Corrected input starts a new request identity against the retained baseline
  unless an explicit matched reset replaced it.

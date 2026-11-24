---
type: REQ
profile-version: 1
id: "REV-REQ-0081"
title: "Failed Library saves support explicit Retry"
---

# Failed Library saves support explicit Retry

## Statement

WHEN a Library metadata or reading-state save fails, the Library MUST preserve the submitted input and the original
[editing baseline](../../api/requirements/0083-editing-baselines-form-only-from-complete-values-of-one-revision.md#statement)
without replaying the save automatically. For a temporary or uncertain failure, the Library MUST offer an explicit Retry
control that sends the retained attempt again under its original editing baseline. For a stale-baseline refusal, the
Library MUST offer an explicit Reload control that fetches current values without sending a write and without offering
an unchanged Retry. For a validation refusal, the Library MUST preserve the submitted input with field feedback for
correction, and for a locked-field or permission refusal it MUST show specific feedback without offering an unchanged
Retry or Reload. For a batch action over multiple books, the Library MUST classify each target independently under its
own editing baseline and Retry MUST send only temporary or uncertain failures again, never successful or conflicted
targets.

## Rationale

A reader recovering from a failed save should neither lose entered input nor silently overwrite changes made elsewhere.
Automatic replay can double-apply writes or race with concurrent edits, while offering unchanged Retry on a stale or
locked target repeats a refusal the reader cannot resolve without reloading or correcting the input. Independent target
classification in batch actions keeps one conflicted book from blocking Retry of temporarily failed siblings or
re-sending already committed updates.

## Acceptance criteria

- A temporary or uncertain metadata or reading-state save failure shows an explicit Retry action, sends no automatic
  replay, shows pending feedback while retrying, and on activation sends the original submitted input again under its
  original editing baseline even if later reads refreshed displayed values. Checked by
  `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.
- A stale-baseline refusal offers an explicit Reload action and no Retry action; activating Reload sends no write,
  updates displayed values with their matching validator, and keeps the failed submitted value visible until the reader
  makes a new edit. Checked by `Library Retry sends the retained edit again` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- A validation refusal retains the submitted input and field feedback so reopening the editor lets the reader correct
  and resubmit against the retained baseline. Checked by `Library Retry sends the retained edit again` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- A locked-field refusal shows specific feedback without offering Retry or Reload. Checked by
  `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.
- A batch Set status action sends each target's own editing baseline, classifies successful, temporarily failed and
  conflicted targets independently, and activating Retry failed books sends only the temporarily failed target again
  under its original baseline while leaving successful and conflicted targets unsent. Checked by
  `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.

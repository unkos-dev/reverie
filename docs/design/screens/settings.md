# Settings screen

Administrators change the instance-wide runtime settings here. The screen is one page per settings area, and each area
saves on its own.

This document is the specification a rendered page is compared against. Copy in double quotes is exact. Where a value is
shown as `{n}`, `{Area}` or similar, the screen fills it in.

## Purpose

- Let an administrator read and change the settings that apply to everyone who uses the instance.
- Take effect without a restart, except for any field the server reports as needing one.
- Protect against two administrators overwriting each other, without making the common case noisy.

## Entry point and routes

The entry point is the "Settings" item in the user menu, shown to administrators only. Other accounts see no item and no
hint. The navigation rail has no Settings item.

| Route                             | Content                                     |
| --------------------------------- | ------------------------------------------- |
| `/admin/settings`                 | Redirects to `/admin/settings/acquisition`. |
| `/admin/settings/acquisition`     | Acquisition area.                           |
| `/admin/settings/enrichment`      | Enrichment area.                            |
| `/admin/settings/covers`          | Covers area.                                |
| `/admin/settings/writeback`       | Writeback area.                             |
| `/admin/settings/catalogue`       | Catalogue feed area.                        |
| `/admin/settings/{anything else}` | Redirects to `/admin/settings/acquisition`. |

Five areas ship. A provider display area, which would let an administrator hide metadata providers from book details, is
not part of this screen yet. No route, sub-navigation item or placeholder stands in for it.

A non-administrator who opens a settings address directly sees the "Settings are for administrators" state. No settings
request is made for them.

## Layout and information architecture

The content sits in a centred column no wider than 1180 px, on the admin zone's second canvas tone, with no atmosphere
effects and no card inside a card.

### Page head

Shown on every area, and shown while the settings load:

1. A breadcrumb in small fixed-width type capitals: "Admin / Settings / {Area}", where {Area} is the area's title. It is
   plain text in the page head.
2. The heading "Settings" (the page's only `h1`).
3. The intro: "Instance settings for this library. They apply to everyone who uses it, and they take effect without a
   restart."
4. The status line, once the settings have loaded:
   - "Last changed", then the time the settings were last saved, for example "9 Oct 2026, 6:31 pm" for an en-AU viewer.
   - "Server cache refreshed", then the time this server last refreshed its copy of the settings and how long ago, for
     example "6:42 pm (4 minutes ago)" for an en-AU viewer. Under a minute reads "just now". When the server has not
     refreshed since it started, the value is "Not refreshed since the server started. Values are read straight from the
     database, so this is normal."
   - If the reload-status read (`GET /api/v1/settings/reload-status`) is still loading or fails, the "Server cache
     refreshed" item is omitted and the settings stay usable.

Times are shown in the viewer's locale.

### Body

Below the head, a two-column grid: a 15 rem sub-navigation on the left, a 3 rem gap, and the open area's form filling
the rest. The sub-navigation stays in view while the form scrolls.

**Sub-navigation.** A `nav` named "Settings areas" with one link per area. Each link shows the area's title and a
one-line summary of its saved values. The current area has `aria-current="page"` and, while it has unsaved edits, an
"Edited" tag beside its title. Summaries come from saved values, never from the draft:

| Area | Summary |
| ---- | ------- |
| Acquisition | "Imports on." or "Imports off.", then "{FORMATS}. Source files kept" when neither removal switch is on. |
| Acquisition | The imports prefix, then "{FORMATS}. Source files removed after import" when only the import switch is on. |
| Acquisition | The imports prefix, then "{FORMATS}. Source files removed" otherwise. |
| Enrichment | "On. {n} lookups at a time" ("1 lookup at a time"), or "Off". |
| Covers | "Up to {n} MiB", with {n} rounded to two decimal places and trailing zeros dropped. |
| Writeback | "On. {n} files at a time" ("1 file at a time"), or "Off". |
| Catalogue feed | "On. {n} entries per page" ("1 entry per page"), or "Off". |

**Area form.** A `form` named "{Area} settings" holding the area's heading (an `h2` with the area's title), its
description, one row per field, and the save bar. Each field row is two columns: the label and help text (up to 24 rem)
on the left, the control and its notes on the right.

**Save bar.** Sits at the foot of the form and at the bottom of the viewport. On an area shorter than the viewport, the
space between the last row and the bar stays empty; on a taller area, the bar sticks to the bottom of the viewport as
the form scrolls. The bar's message sits on the left. "Discard changes" and the single primary button "Save changes" sit
on the right.

## Areas and fields

Each area has a description under its heading.

### Acquisition

"What Reverie accepts from the ingestion folder, and what it does with the source files afterwards."

- **Import files.** The first field, a switch. Help: "When off, Reverie stops starting new imports. Work already started
  can finish, and scans and recovery continue. The setting takes effect when you save." It uses the area's draft,
  Discard changes and Save changes flow, with no confirmation or restart. The format and cleanup controls stay usable
  while it is off. A successful save containing `ingestion_enabled` invalidates ingestion status without writing a
  blocker snapshot or reading settings again beneath an edited draft.
- **Accepted formats.** While EPUB is the only supported format, this row is the read-only line "Reverie imports EPUB
  files." It has no checkbox, no help text and no part in a save. Once more formats are supported, the row becomes a
  checkbox list labelled "Accepted formats", one checkbox per format, and a save sends the whole list. The last enabled
  format cannot be turned off: its checkbox stays checked and in the tab order with native `isReadOnly`, and the note
  "This is the only format selected. At least one format must stay selected, or Reverie would have nothing to import."
  appears beneath the list and is tied to the checkbox as its description. Enabling a second format removes the lock and
  the note.
- **Remove the source file after a successful import.** A switch. Help: "Deletes the original file from the ingestion
  folder once the book is in the library. This cannot be undone." Turning it on asks first (see the removal
  confirmation).
- **Remove the source file when the book is a duplicate.** A switch. Help: "Deletes the original file from the ingestion
  folder when the library already holds the same book. This cannot be undone." Turning it on asks first.

### Enrichment

"Looks up metadata for your books from external sources."

- **Fetch metadata from external sources.** A switch. Help: "When off, no new lookups run and books keep the metadata
  they arrived with."
- **Concurrent lookups.** A whole number from 1 to 10, unit "lookups". Help: "How many books are looked up at the same
  time. Higher is faster but sends more requests to external sources. Between 1 and 10."
- **Idle check interval.** A whole number of 1 or more, unit "seconds". Help: "How long the enrichment worker waits
  before checking an empty queue again."
- **Time allowed per source.** A whole number of 1 or more, unit "seconds". Help: "The longest Reverie waits on one
  external source for one book."

### Covers

"Limits applied when Reverie downloads cover images."

- **Largest cover file.** A size in MiB, which may be fractional and must be greater than 0. Help: "Covers larger than
  this are rejected. Stored as bytes; the exact value is shown below the field." Beneath the input, in fixed-width type,
  the exact byte count with the viewer's locale digit grouping, for example "10,485,760 bytes". It follows the input as
  the user types and shows the saved byte count while the text is not a valid size. A size is converted to the nearest
  whole byte.
- **Download timeout.** A whole number of 1 or more, unit "seconds". Help: "How long Reverie waits for a cover to finish
  downloading."
- **Smallest acceptable cover.** A whole number of 1 or more, unit "pixels". Help: "Covers whose longest side is shorter
  than this are rejected."
- **Redirects to follow.** A whole number of 0 or more, unit "redirects". Help: "How many redirects Reverie follows when
  fetching a cover. 0 follows none."

### Writeback

"Writes corrected metadata back into the book files."

- **Write corrections into book files.** A switch. Help: "When on, Reverie rewrites the metadata and cover inside each
  EPUB to match the library record, and restores the file if the result fails validation."
- **Concurrent writes.** A whole number from 1 to 10, unit "files". Help: "How many files are rewritten at the same
  time. Between 1 and 10."
- **Idle check interval.** A whole number of 1 or more, unit "seconds". Help: "How long the writeback worker waits
  before checking an empty queue again."
- **Attempts per file.** A whole number of 1 or more, unit "attempts". Help: "How many times Reverie retries a failed
  write before it gives up on that file."

### Catalogue feed

"The OPDS feed that reading apps use to browse and download your books."

- **Serve the OPDS catalogue.** A switch. Help: "When off, reading apps can no longer reach the feed."
- **Entries per page.** A whole number from 1 to 500, unit "entries". Help: "How many books each page of the feed lists.
  Between 1 and 500."

### Controls

- A switch shows a visible "On" or "Off" word beside it, and its accessible name is the field's label. State is never
  carried by colour alone.
- A number input is a text input with a numeric keyboard hint, about 7.5 rem wide, with the unit beside it. Digits align
  in a tabular figure style.
- A whole-number field with no stated maximum accepts values up to 2,147,483,647.
- All controls are read-only or disabled while a save is in flight.

### Validation messages

These appear beneath the field, with a warning glyph, when the user changes the field and leaves it, or attempts a save,
with a bad value:

| Case                                   | Message                                              |
| -------------------------------------- | ---------------------------------------------------- |
| Empty or not numeric                   | "Enter a number."                                    |
| Whole number with both bounds          | "Enter a whole number from {min} to {max}."          |
| Whole number, minimum 1, no maximum    | "Enter a whole number of 1 or more."                 |
| Whole number, minimum 0, no maximum    | "Enter a whole number of 0 or more."                 |
| Above 2,147,483,647                    | "Enter a whole number no larger than 2,147,483,647." |
| Size not greater than 0 or not decimal | "Enter a size greater than 0."                       |
| Size too large                         | "Enter a size no larger than 8,589,934,591 MiB."     |

## Draft and save rules

- One draft belongs to the open area. It holds only the fields the user changed. Only these change it: user input,
  "Discard changes", a successful save, resolving a conflict, and leaving the area after the discard prompt.
- A field whose edited value equals the saved value is not dirty. Editing a field back to its saved value clears its
  edited state.
- The area's bar, row tags and sub-navigation tag count that area's edits only. Edits never carry to another area.
- A save sends the dirty fields as a merge patch and carries the revision of the settings the draft is based on in
  `If-Match`. An empty patch is never sent.
- The `PUT` response is the new base. There is no refetch after a save.
- The settings snapshot is read when an area is entered, fresh each time. It is not read again on window focus or
  reconnect, so the saved values never change underneath a draft. The 412 path below is the reconciliation point.
- "Discard changes" reverts the open area's edits at once, with no confirmation. The now-pristine actions become
  disabled.
- Pressing Enter in a field saves, and keeps focus in that field.
- "Discard changes" and "Save changes" use native `isDisabled` when the area has no edits. During a save, Save uses
  `isPending` to retain focus and suppress repeat activation; Discard is disabled. Idle conflict actions remain
  available even when there are no ordinary dirty fields.

### Edited rows

A changed field's row is tinted with the accent's soft tone, shows an "Edited" tag after its label, and shows "Saved
value: {value}" beneath its control. {value} reads as the control would: "On" or "Off", "EPUB", "10 MiB", or a number
with its unit such as "4 lookups".

### A stale save

The server rejects a save whose revision is out of date with 412 (or 428 when the tag is missing). The screen handles
both the same way:

1. The "Settings changed while you were saving" state appears and controls are disabled. The latest settings are
   reloaded.
2. Each dirty field is compared with the value the draft was based on.
3. If no dirty field changed on the server, the same patch is sent once more against the new revision. On success the
   "Settings saved" state appears with the reapply paragraph.
4. If a dirty field changed on the server to the same value the user chose, that is not a conflict. The field leaves the
   patch and counts as saved. If nothing remains to send, no request is made and the "Settings saved" state appears with
   the reapply paragraph.
5. If a dirty field changed on the server to a different value, nothing more is sent and the conflict panel opens.
6. If the second send is also rejected as stale, the settings are reloaded again and the conflict panel opens against
   the then-current values.
7. If a reload fails, the failure state appears with the edits kept, and the next save reloads before it sends anything.

A field the other administrator changed that is not on the open page is named in the saved notice only.

## States

Each state gives its trigger and its exact copy.

### Loading

- Trigger: the first settings read, or the identity check, is pending.
- The page head shows at once. Below it is a busy status region reading "Loading settings", with skeleton blocks shaped
  like three sections of three rows. Skeleton motion stops under reduced motion.

### Pristine

- Trigger: loaded, no edits.
- Bar message: "No unsaved changes." "Discard changes" and "Save changes" are inactive.

### Dirty

- Trigger: any field differs from its saved value.
- Rows are tinted and tagged "Edited" with "Saved value: {value}". The sub-navigation item is tagged "Edited".
- Bar message, in bold: "{n} unsaved change" or "{n} unsaved changes". Both buttons are active.

### Saving

- Trigger: a save is in flight.
- Controls are read-only or disabled. The primary button reads "Saving" and uses native `isPending`. The bar message is
  a status reading "Saving {n} change" or "Saving {n} changes".

### Saved

- Trigger: a save succeeded. A status notice titled "Settings saved":
  - "Changes are in effect now. No restart is needed."
  - When the save changed a field that needs a restart: "Restart Reverie for {field labels} to take effect. Everything
    else is in effect now." Labels are joined with " and ".
- The bar message is a status: "Saved at {time}." The notice and the bar message clear on the next edit.

### Reapplying

- Trigger: a stale save with no dirty field changed on the server.
- A status notice titled "Settings changed while you were saving": "Another admin saved settings first. Reverie is
  loading the latest settings and applying your changes to them. Nothing needs doing." Controls are disabled and the bar
  reads as for Saving.

### Saved after reapply

- Trigger: a stale save was reconciled without a conflict.
- The "Settings saved" notice gains a leading sentence: "Another admin saved settings while you were editing. None of
  your changes overlapped with theirs, so Reverie reloaded the latest settings and saved your changes on top."
- When the other administrator also changed settings on other pages, a second paragraph reads "Also changed by someone
  else: {Area}, {field label}, now {value}; ...". It lists at most three, then ends ", and {n} more."
- Fields on this page that the other administrator changed show the new value, a "Changed elsewhere" tag and "Changed by
  someone else while you were editing. It was {old value}." These clear on the next edit or when the area changes.

### Conflict

- Trigger: a dirty field was changed on the server to a different value.
- An alert titled "Settings changed while you were editing": "Another admin saved settings after you opened this page,
  so yours were not saved. Your edits are still here. Where you both changed the same setting, choose which value to
  keep. Settings only they changed now show their value."
- Each overlapping row is tagged "Conflict" and carries a group with the legend "Someone else saved {theirs} while you
  were editing. You set {mine}." and two radio options: "Keep mine ({mine})" (selected by default) and "Use theirs
  ({theirs})".
- Rows only the other administrator changed carry the "Changed elsewhere" tag and note described above.
- Editing a conflicting field again switches its choice back to "Keep mine" and refreshes {mine}.
- Bar message: "Not saved." in bold, then "{n} setting was changed by someone else." or "{n} settings were changed by
  someone else." When the count is zero: "Settings were changed by someone else."
- The buttons become "Discard mine and reload" and the primary button "Save my choices".
- "Save my choices" sends the fields set to "Keep mine" against the new revision. Fields set to "Use theirs" drop out of
  the draft. If every choice is "Use theirs", nothing is sent and the panel clears.
- "Discard mine and reload" drops the draft and reads the settings again.
- The draft still counts as unsaved while the panel is open, so leaving raises the discard prompt.

### Validation

- Trigger: a client check fails, or the server answers 422 with a detail that starts with a field's key.
- Each failing field has `aria-invalid`, a warning glyph and its message beneath the control, which describes the
  control. A server message shows as the server wrote it.
- A save that a client check blocks sends nothing and moves focus to the first failing field. No alert appears and the
  bar keeps its unsaved count.
- A 422 that names a field shows an alert titled "Settings were not saved" with the body "Review the field errors and
  try again." Focus stays where it was.
- A 422 that names no field shows the same alert with the body "The server did not accept this change: {detail}." and
  marks no field.
- Save stays active. Changing a field and leaving it checks it again and clears a server error on it. The alert stays
  until the next save, "Discard changes" or leaving the area.

### Removal confirmation

- Trigger: either removal switch goes from Off to On. The switch stays Off until the user confirms. Turning a switch
  Off, or turning it back Off before saving, needs no dialog.
- An alert dialog:
  - For the import switch: title "Remove source files after import?", body "Once a book is in the library, Reverie will
    delete the original file from the ingestion folder. This cannot be undone. The setting takes effect when you save."
  - For the duplicate switch: title "Remove source files for duplicates?", body "When the library already holds a book,
    Reverie will delete the duplicate original file from the ingestion folder. This cannot be undone. The setting takes
    effect when you save."
  - Actions: "Keep source files" (outline, focused when the dialog opens) and "Turn on removal" (solid danger fill with
    a warning glyph). There is no typed confirmation.
- Keep and Escape both leave the switch Off and return focus to the switch.
- Confirming turns the switch On, tags the row "Edited" and shows a status note in the row: "When you save, Reverie
  starts deleting the original file from the ingestion folder {after a successful import | when the library already
  holds the same book}. This cannot be undone." Nothing is saved until the user saves.

### Discard prompt

- Trigger: the user tries to leave the area while it has unsaved edits. That covers the sub-navigation, any other in-app
  link (rail, user menu, breadcrumb) and the browser's back and forward buttons. Moving within the same path does not
  raise it. It is not raised when the area is pristine, after a successful save, or after the "Nothing was saved" state.
- An alert dialog titled "Leave without saving?" with the body "You have {n} unsaved change in {Area}. It will be lost
  if you leave." or "You have {n} unsaved changes in {Area}. They will be lost if you leave."
- Actions: "Stay on this page" (outline, focused when the dialog opens) and "Leave and discard changes" (outline). The
  dialog has no primary button and no danger styling.
- Stay and Escape close the dialog and return focus to the control that was focused before it opened. Leave moves on and
  clears the draft, so coming back shows the saved values.
- Closing or reloading the tab while the area has unsaved edits shows the browser's own prompt, which the page cannot
  style or reword.

### Restart required

- Trigger: the server lists a field as needing a restart. The state exists only when the data says so.
- The row carries a dashed "Restart required" tag and the hint "Takes effect after Reverie restarts." The saved notice
  names the field as described under Saved.

### Nothing was saved (403 on save)

- Trigger: the save is refused with 403.
- An alert titled "Nothing was saved": "Your account no longer has administrator access. Your edits are still on this
  page. Ask an administrator to restore your access or to make the change for you." Action: an outline button "Leave
  settings", which goes to the library without the discard prompt.

### Save failed

- Trigger: the save fails for any other reason.
- An alert titled "Settings were not saved" with the body "The server could not be reached. Your edits are still here."
  when the request did not complete, or "The server returned an error. Your edits are still here." when the server
  answered with an error. The user can save again.
- A 401 on save is handled by the application's sign-in flow. The edits stay and the screen shows no notice of its own.

### Load error

- Trigger: the settings read, or the identity check, fails.
- An alert titled "Settings could not be loaded": "The server did not respond, or it returned an error. Nothing has been
  changed." Action: an outline "Try again", which repeats the failed read.

### Forbidden

- Trigger: a non-administrator opens a settings address, or the settings read answers 403 for an administrator whose
  role has lapsed.
- An alert titled "Settings are for administrators": "Your account does not have administrator access. Ask an
  administrator if something needs changing." Action: an outline link "Back to the library".

## Keyboard, focus and accessibility

- Tab order: the shell, the sub-navigation, the form fields in document order, then "Discard changes" and the save
  button when those actions are enabled.
- Each area form is named "{Area} settings". The area title is an `h2` under the page's `h1`.
- Status notices ("Settings saved", the reapplying notice, the removal note and the bar message) are `role="status"`.
  The conflict, validation, nothing-was-saved, save-failed, load-error and forbidden notices are `role="alert"`.
- A save blocked by a client check moves focus to the first failing field. A server refusal leaves focus where it was,
  and its alert announces it.
- The save button stays in the tab order while pending and rejects repeat activation. Pristine actions and Discard
  during a save are disabled and leave the tab order; no application focus transfer runs when they become disabled.
- Dialogs trap focus, close on Escape as the safe choice (Stay or Keep), and return focus as described above. The
  backdrop uses the overlay colour; the dialog uses the surface colour with rounded corners and no shadow.
- Exactly one primary button is on the page: "Save changes", or "Save my choices" during a conflict. Everything else is
  outline or ghost.
- One global focus ring applies to every control, in both themes.
- Under reduced motion the skeleton pulse stops and the switch thumb moves without a transition.
- No state is carried by colour alone. The danger colour appears only on the removal confirmation's confirm button and
  on invalid-input borders, and each is paired with a glyph or text.
- Copy uses Australian English, with no exclamation marks.

## Acceptance checklist

The first Acquisition field is Import files. Its draft does not change the saved summary or ingestion status; saving
updates the summary and invalidates status. Failed and stale saves retain the existing reconciliation behaviour.

A rendered page is accepted when every item holds, in the dark theme and the light theme, at 1920 by 1080 or larger.

1. The page head, status line, sub-navigation, area form and save bar match the layout above, with the container no
   wider than 1180 px.
2. `/admin/settings` and an unknown area redirect to Acquisition. The five areas render the fields, bounds, units,
   labels and help text listed above, and no provider display area or placeholder appears.
3. The sub-navigation summaries match the table and follow the saved values.
4. An administrator sees "Settings" in the user menu. A non-administrator sees no item and no hint, and a direct visit
   shows the Forbidden state without a settings request.
5. While EPUB is the only supported format, Acquisition shows the line "Reverie imports EPUB files." and no checkbox or
   help text. With more formats, the last enabled format is read-only and in the tab order, and the note describes it.
6. The Covers field shows the exact byte count beneath the input and keeps it in step with typing.
7. Editing a field shows the Edited row, the Edited sub-navigation tag, "Saved value: {value}" and the unsaved count in
   the bar. Editing back to the saved value clears all of them. Edits in one area never show in another area's bar.
8. "Discard changes" reverts without a dialog. Pristine actions are disabled; Save retains focus while pending and
   rejects repeat activation. Idle conflict actions remain available.
9. A save sends only the dirty fields with the settings revision in `If-Match`, never an empty patch, and the response
   becomes the base with no refetch.
10. A forced stale save with no overlapping field reloads, sends once more and shows "Settings saved" with the reapply
    paragraph. A server value equal to the user's own is not a conflict. An overlapping field opens the conflict panel
    with Keep mine and Use theirs, and a second stale answer opens the panel too.
11. Validation shows the message on the field. A save blocked by a client check sends nothing, shows no alert and
    focuses the first failing field. A 422 naming a field shows "Review the field errors and try again." without moving
    focus. A 422 and a network failure keep the user's edits.
12. Leaving with unsaved edits raises the discard prompt with "Stay on this page" focused. It is not raised when
    pristine, after a save, or after "Nothing was saved".
13. Turning on either removal switch raises the confirmation with "Keep source files" focused. Keep returns focus to the
    switch and leaves it Off. Confirming shows the Edited row and the removal note. Turning a switch Off raises nothing.
14. The reload-status line reads as specified, including the never-refreshed value, and disappears when that read fails.
15. A keyboard-only pass reaches every control, dialogs trap and return focus, and one focus ring shows throughout.

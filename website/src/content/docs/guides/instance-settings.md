---
title: Instance settings
description: How administrators change Reverie's runtime settings from the Settings screen, and what happens when two administrators edit at once.
---

Administrators tune Reverie from **Settings** in the user menu at the foot of the navigation rail. The settings apply to
everyone who uses the instance, and they take effect without a restart. Other accounts see no Settings entry, and
opening the address directly shows a notice that the screen is for administrators.

## Areas

Each area is its own page with its own **Save changes** button, and the list on the left summarises what each one is
currently set to.

| Area           | What it controls                                                                             |
| -------------- | -------------------------------------------------------------------------------------------- |
| Acquisition    | Which formats the ingestion folder accepts, and whether source files are removed afterwards. |
| Enrichment     | Whether metadata lookups run, how many at once, and how long each source is given.           |
| Covers         | The largest cover file, download timeout, smallest acceptable size and redirect limit.       |
| Writeback      | Whether corrected metadata is written into book files, and how many files at once.           |
| Catalogue feed | Whether the OPDS feed is served, and how many entries each page lists.                       |

Provider display, which hides providers from book details, follows in a later release.

The status line under the heading shows when the settings last changed and when the server last refreshed its copy. A
server that has not refreshed since it started reads values straight from the database, which is normal.

## Saving

Edited rows are highlighted and show the saved value beneath them. **Save changes** sends only the fields you changed,
and **Discard changes** puts the area back as it was. Numbers are checked against the same limits the server enforces,
so a mistake is marked on the field before anything is sent.

Edits belong to the area you are on. Moving to another area, or leaving Settings, with unsaved edits asks first. Choose
**Stay on this page** to keep editing or **Leave and discard changes** to drop them.

Turning on either switch that removes source files asks for confirmation before the switch changes, because the files
are deleted from the ingestion folder and cannot be recovered. The switch stays off until you choose
**Turn on removal**, and nothing is saved until you then save the area.

## When two administrators edit at once

Every save is checked against the version of the settings you loaded. If another administrator saved first, Reverie
loads their changes and compares them with yours:

- If none of the settings you edited were changed by them, Reverie applies your edits on top of theirs and saves once. A
  notice says what the other administrator changed.
- If they set one of your edited settings to the same value you chose, there is nothing to resolve and it is treated as
  saved.
- If they changed a setting you also edited, nothing is saved. The conflicting rows offer **Keep mine** or
  **Use theirs**, settings only they changed show their new value, and **Save my choices** saves what you picked.
  **Discard mine and reload** drops your edits instead.

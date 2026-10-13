# Admin ingestion screen

The ingestion page shows an administrator whether the ingestion folder is flowing into the library, what needs attention
and why, and which books have metadata lookups failing. A scan control starts a fresh look at the folder. The library
page's empty state hands administrators on to this page, and its copy is specified at the end of this document.

This document is the specification a rendered page is compared against. Copy in double quotes is exact. Where a value is
shown as `{n}` or similar, the screen fills it in.

Three parts are approved but not yet implemented: the paused-pipeline notice, "Retry all" in group headers and the
Duplicates card. They are collected under "Target design, not yet built", along with two smaller differences. Everything
outside that section specifies the page as it is to behave, except where a line points to that section.

## Purpose

- Make a stuck or failing ingestion visible without opening a log.
- Group failures by why they happened, so each group carries one action sentence.
- List every affected file or book once, under its main reason, with a relative path or title and when it was last
  tried.
- Keep the existing library-health metric cards where they are.

## Entry point and route

- Route: `/admin/dashboard`. The navigation label is "Ingestion" and the page heading is "Library health".
- Administrators only. A non-administrator is redirected to the library, and no ingestion request is made for them.
- The library page's empty state links here with "Open Ingestion" or "Review these files".

## Layout and information architecture

The content sits in a centred column no wider than 1152 px (72 rem), with 1.5 rem padding, on the admin zone's canvas.
From top to bottom:

1. **Page head.** The `h1` "Library health" and the intro "Reverie watches the ingestion folder and files each new EPUB
   into the library. A scan checks every file again and retries files that were set aside for another try." The scan
   control sits at the top right.
2. **Status region.** One polite status region holding the scan feedback, the latest-activity line and the after-scan
   notice. It is always present and empty on a fresh load.
3. **Needs attention card.** Files Reverie could not import, in groups.
4. **Enrichment problems card.** Books whose metadata lookups are failing, in groups.
5. **Existing metric cards,** unchanged: the Books, Works, Storage and Degraded totals, Validation, Enrichment, Metadata
   coverage, Storage by format, and Recent ingestion batches. They keep their own number and date formatting.

The Duplicates card and the paused-pipeline notice, when built, take the positions given under "Target design, not yet
built".

Counts on this page use the viewer's locale digit grouping. Times are shown in the viewer's locale.

## Scan control

- A primary button, "Scan ingestion folder", with the hint "Checks every file in the folder." beneath it. It is the only
  primary button on the page.
- While a request is pending the button reads "Scanning", shows a spinner and is marked `aria-disabled` so it can still
  take keyboard focus. A second press sends nothing.
- The button stays enabled after a successful scan. It is disabled for the rest of the page session after a 403.

## Scan feedback

Each state is one line in the status region: an icon, a bold title and one detail sentence. No state uses a danger
colour. Failures are boxed and are alerts.

### Requesting

- Trigger: the scan request is pending.
- "Scanning the ingestion folder" / "Looking at every file. This usually takes a few seconds."

### Accepted

The scan answers with three counts: queued, waiting (deferred) and unchanged (suppressed). Zero counts keep their
figures where the title carries them.

- Any count above zero for queued: "Scan started. {q} queued, {d} waiting, {s} unchanged." / "Queued files are being
  imported now. Waiting files are either still being written or waiting for a retry time, and are picked up
  automatically. Unchanged files were left as they are."
- All three zero: "Nothing to import" / "The folder has no new or changed files."
- Queued and waiting zero, unchanged above zero: "Nothing new to import" / "{s} files were left as they are, because
  nothing about them has changed." For one file: "1 file was left as it is, because nothing about it has changed."
- Queued zero, waiting above zero: "Nothing is ready yet" / "{d} files are waiting and will be picked up automatically.
  You do not need to scan again." For one file: "1 file is waiting and will be picked up automatically. You do not need
  to scan again."

No count on this page is labelled as the result of a particular scan. Batch figures always say "the latest batch".

### Latest activity

- Trigger: the latest batch has not ended, and no scan request is pending.
- A line under the status: "Latest activity. {t} files in the latest batch: {i} imported, {f} did not import, {p} in
  progress. Updating every few seconds." A "{k} skipped" part appears after "did not import" only when {k} is above
  zero. {t} reads "1 file" for one.

### Latest activity finished

- Trigger: a running batch ends, or, after a scan, a newer batch has ended.
- "Latest activity has finished" / "{t} files in the latest batch: {i} imported, {f} did not import. Batches are not
  tied to a particular scan. Anything that did not import is under Needs attention." The skipped part follows the same
  rule as above.

### After a scan

- Trigger: the counts refreshed after a scan show fewer files under Needs a change and Failed after several tries
  together than before the scan.
- "Files were sent for another try" / "Files marked Needs a change are back in the queue and return here if they fail
  again. Rejected files were left as they are, because they have not changed."
- The title carries no number. The Needs attention card shows the same message in short form (see below). The notice is
  not shown when those counts did not fall.

### Scan failures

These are alerts, shown beside the status region, in a bordered box with an icon and no danger colour. They stay until
the next scan starts. A batch finishing does not clear them.

- Request failed (5xx or the network): "The scan did not start" / "Reverie could not complete the request, so nothing
  was changed. Try again. If it keeps happening, check the server log." with an outline "Try again" button.
- Forbidden (403): "This account cannot start a scan" / "Sign in as an administrator to scan the ingestion folder." The
  scan button is disabled.
- Session ended (401): "Your session has ended" / "Sign in again to scan the ingestion folder."

There is no dismiss control. The next scan state replaces the notice.

## Needs attention card

### Head

- The heading "Needs attention" and, in fixed-width type beside it, "{n} files, {m} reasons" ("1 file", "1 reason" in
  the singular). {n} is the attention total and {m} is the number of attention groups shown. The ignored group is in
  neither.
- The intro: "Files Reverie could not import, grouped by why. Each original stays in the ingestion folder, unchanged."
- When the after-scan condition holds, a muted line follows the intro: "Files marked Needs a change were sent for
  another try. They return here if they fail again. Rejected files stay until the file changes."

### Groups

A file has one primary reason and appears in exactly one group, under that reason. Any other reasons it has show on its
row as an "Also" line. The group counts add up to the attention total, and secondary reasons never add to any count.
Only groups with a count above zero are shown, in this order: the rejected classes by severity, then the needs-a-change
classes, then the ignored group last.

| Class | Group heading | Action sentence | Tag |
| ----- | ------------- | --------------- | --- |
| `unsafe_contents` | "Unsafe contents" | "Reverie will not import this file. Remove it, or get the book from a source you trust." | "Rejected" |
| `damaged` | "The file is damaged" | "Download or export the book again, then scan." | "Rejected" |
| `invalid_structure` | "Not a valid EPUB" | "Try another copy, or repair it in an EPUB editor, then scan." | "Rejected" |
| `over_limits` | "Over Reverie's limits" | "Remove it, or try a leaner edition of the book." | "Rejected" |
| `unspecified` | "Reason not specified" | "Replace the file, or check the server log, then scan." | "Rejected" |
| `needs_change` | "Needs a change" | "Check permissions and free space on the ingestion and library folders, then scan." | "Needs a change" |
| `retries_exhausted` | "Failed after several tries" | "Check that the file opens normally, then scan to try again." | "Needs a change" |
| `format_not_accepted` | "Ignored: format not accepted" | "Images and other non-EPUB files. They need no action. Not counted above." | "Ignored" |

Older records whose reason was never classified show as "Reason not specified". The page never shows raw reason text.

**Group header.** The whole header is one button with `aria-expanded` and `aria-controls`. Left to right: a chevron that
points down when open and right when closed, the bold group heading with its action sentence beneath, the status tag in
fixed-width type capitals, and the file count ("2 files", "1 file"). The ignored group is muted: its heading is not
bold. Rejected tags are bold.

**Initial expansion.** On the first load of the counts, the first attention group is expanded and the others are
collapsed. The page opens on real files. Later refreshes do not change what the user has opened or closed.

**Rows.** An expanded group is a table named "Files: {group heading}". Each row has:

- The file's path relative to the ingestion folder, in fixed-width type. The directory part is muted and the file name
  is not, for example "incoming/older/" then "Dunmore.epub". A path is never absolute. A file name that is not valid
  UTF-8 shows with its escape sequences.
- When the file has other reasons, a second line "Also: {labels}" using the other groups' headings, joined with "; ".
  For example "Also: Not a valid EPUB". The secondary reasons are never shown as rows in their own groups. (The
  separator is a target-design difference; see "Smaller differences from the page as built".)
- On the right, the time the file was last tried, for example "9 Oct 2026, 4:12 pm" for an en-AU viewer. A file never
  tried shows nothing.

**Footer.** Each expanded group ends with "Showing {n} of {total}" on the left and, when more rows exist, an outline
"Show more" on the right. Pages hold 25 rows. The next page is requested by its own cursor and the loaded rows stay.

- While a page loads the button reads "Loading more", shows a spinner and is marked `aria-disabled`, so it stays in the
  tab order.
- After a page loads, a polite status announces "Loaded {n} more files" ("1 more file"). Focus stays on "Show more"
  while more rows remain. When the last page loads, focus moves to the count line instead of being dropped.
- If a later page fails, the footer shows the alert "Could not load these files." and an outline "Try again". Loaded
  rows stay. Focus moves to "Try again", and after a successful retry to the count line.

### States

- **Loading.** The first counts read is pending. The card is busy and holds three skeleton lines, with the status text
  "Loading files that need attention" for screen readers. Skeleton motion stops under reduced motion.
- **First page loading** (an expanded group): a busy skeleton with the status text "Loading files".
- **First page failed:** the alert "Could not load these files." and "Try again". Pressing it moves focus to the group
  header.
- **Counts failed.** "Could not load this list." / "Your files are untouched. Try again." with an outline "Try again".
  There is no partial fallback, because groups cannot be drawn without counts.
- **Empty.** No attention group has files. The head has no total and the intro reads "Files Reverie could not import,
  grouped by why." The body reads "Nothing needs attention." / "Every file Reverie has seen was imported, is waiting its
  turn, or was ignored." The ignored group still shows below, when it has files.
- **Ignored group.** Collapsed on load, muted, last, and outside the total. Expanding it requests the
  `format_not_accepted` class.

## Enrichment problems card

Books whose latest metadata lookup failed, grouped by source and reason. Each book is listed once, under its most recent
failing source. Any other failing source appears on its row.

### Head

- The heading "Enrichment problems" and, in fixed-width type, "{n} books, {m} reasons" ("1 book", "1 reason").
- The intro: "Books where the metadata sources could not be reached or returned nothing usable, grouped by source and
  reason. Each book is listed once, under its main reason."

### Groups

A group is a source plus a failure class. The heading is "{Source}: {class}", for example "Open Library: timed out". A
failure that belongs to no source uses the class's capitalised name alone, for example "Timed out". Source names are the
metadata sources' display names: "Open Library", "Google Books" and "Hardcover". An unknown source key shows as itself.
The group's explanation sits beneath its heading, in the place of the action sentence. Groups carry no status tag. The
first group is expanded on load.

| Class | Name | Explanation |
| ----- | ---- | ----------- |
| `timeout` | "Timed out" | "The source did not answer in time. Usually temporary." |
| `rate_limited` | "Rate limited" | "The source asked Reverie to slow down. Reverie waits and tries again." |
| `source_error` | "Source error" | "The source reported a problem. Usually temporary." |
| `not_found` | "No record found" | "The source has nothing for this book. Check the book's ISBN or title." |
| `unreachable` | "Could not be reached" | "The request failed or the reply could not be read. Check the server's network access." |
| `internal` | "Internal error" | "Reverie hit a problem of its own. Check the server log." |
| `unspecified` | "Reason not specified" | "Older records land here." |

In a heading the class name is lower case after the source: "Hardcover: no record found".

### Rows

An expanded group is a table named "Books: {group heading}". Each row has:

- The book's title, bold and underlined, linking to the book's page.
- Beside it, muted, the status and attempt count: "Will retry, tried {n} times" while Reverie is still retrying, or
  "Stopped retrying, tried {n} times" once it has given up ("tried 1 time" for one).
- When the book also fails at other sources, a line "Also: {Source}, {class}; {Source}, {class}", for example "Also:
  Hardcover, no record found".
- On the right, an outline "Try again" button.

The row also carries two times, described under "Smaller differences from the page as built".

### Retrying one book

- Pressing "Try again" changes the button to "Queuing" with a spinner, marked `aria-disabled`. A second press sends
  nothing.
- On success the button is replaced by the status "Queued. It leaves this list once it runs." Focus moves to it when
  focus would otherwise be lost.
- On failure the row shows the alert "Could not queue it." beside the "Try again" button.
- The list is not read again at the moment of a retry, so the row and its confirmation stay in place. The list refreshes
  on the next window focus or visit. If the book fails again, a newer attempt shows on its row and the button returns to
  "Try again".

### States

- **Loading:** a busy skeleton with the status text "Loading books with enrichment problems".
- **Empty:** the heading shows no total, with "No enrichment problems." / "Every book was enriched, or is waiting its
  turn."
- **Counts failed:** "Could not load this list." / "Your books are untouched. Try again." with "Try again".
- **Group loading:** "Loading books". **Group failed:** the alert "Could not load these books." with "Try again". A
  later page failing uses the same alert in the footer.
- **Footer:** "Showing {n} of {total}", "Show more", "Loading more", and the announcement "Loaded {n} more books" ("1
  more book"), behaving as for files.

## Refresh behaviour

One part of the page owns all polling, and no other component starts a timer.

- Counts, the latest activity and the first page of every expanded group refresh every 3 seconds while the latest batch
  has not ended, and for 2 minutes after a scan that queued or deferred any file. Then polling stops.
- Collapsed groups make no requests. Later pages, once opened, are never polled.
- Window focus refreshes the counts, the activity and the first page of each expanded group, whether or not polling is
  running.
- The enrichment lists are not polled. They refresh on window focus.

## Empty library guidance

This is on the library page, not this page, and applies when the library holds no books and no filter is active. It sits
inside a dashed-border panel centred in the content area. The guidance reads the ingestion counts and the latest batch
once, and refreshes them on window focus only, because this page's poll owner is the only one that polls.

- **Reader** (an adult or child account): "No books yet" / "There are no books here yet. Ask the person who manages this
  library to add some." No action, and nothing about administration.
- **Administrator, checking.** While the two reads are pending: a busy skeleton with the status text "Checking the
  ingestion folder". If either read fails, the default guidance shows instead.
- An administrator sees the first of these that applies: under way, then needs attention, then default.
- **Administrator, ingestion under way** (the latest batch has not ended): "Ingestion is under way" / "{t} files are
  being checked. Books appear here as each one finishes." ("1 file is being checked.") Beneath it "{i} imported, {f} did
  not import, {p} in progress" and a progress bar named "Files imported in the latest batch", with the value text "{i}
  of {t} files imported". An outline link "Open Ingestion" goes to `/admin/dashboard`.
- **Administrator, files need attention** (the attention total is above zero; ignored files do not count): "No books
  yet" / "{n} files in the ingestion folder could not be imported. The Ingestion page shows why and what to do about
  each." ("1 file in the ingestion folder could not be imported.") A primary button-styled link "Review these files"
  goes to `/admin/dashboard`. It is the page's only primary action.
- **Administrator, default:** "No books yet" / "Add EPUB files to the ingestion folder on your server. Reverie watches
  the folder and files each book it can read." Three numbered steps follow, each a bold title and one sentence:
  1. "Where files go":
     `"Copy or move EPUB files into the ingestion folder. Subfolders are fine. The folder is set when Reverie is deployed."`
  2. "How it runs": "Reverie picks up a file once it has stopped changing, checks it, repairs it where it can, and files
     it. Books appear here as they finish."
  3. "What to expect": "Large batches take a while. Files Reverie cannot import stay in the folder, untouched, and are
     listed on the Ingestion page with the reason."

  Then an outline link "Open Ingestion" to `/admin/dashboard`. The copy never shows a folder path.

## Target design, not yet built

This section is the approved design for what comes next. It is marked apart so a render is not failed for lacking it.
When a part is built, it moves into the sections above.

### Smaller differences from the page as built

- **"Also" separator for files.** Secondary reasons on a file row are joined with "; ", as enrichment rows already are.
  The built page joins file labels with ", ", which differs only for a file with three or more reasons.
- **Enrichment times.** Each enrichment row shows the time of the book's most recent attempt. When a source has asked
  Reverie to wait, it also shows the earliest retry time, only when that field is present. The labels are not yet
  settled. The built page shows neither.

### Paused-pipeline notice

The notice appears only when ingestion is paused or has stopped. A healthy page, and a page still checking, look exactly
as described above.

- **Placement.** A boxed notice above the status region, in the normal flow, with no danger colour.
- **Paused.** Title "Ingestion is paused". Beneath it, the reason sentence from the table below, then a bold "What to
  check." and its check text, then "Paused since {time}. Reverie checks again every few minutes and carries on by itself
  once the problem is fixed. Files already in the queue are kept."
- **Stopped.** Title "Ingestion has stopped". Beneath it "The ingestion worker is no longer running, so no files will be
  imported." then "What to check." and "Restart Reverie. If it stops again, check the server log."

### Paused reasons

A paused notice carries exactly one reason:

| Reason | Sentence | What to check |
| ------ | -------- | ------------- |
| Database unavailable | "Reverie cannot reach the database that ingestion uses." | "Check that the database is running and that the ingestion database account can sign in." |
| Ingestion folder unavailable | "Reverie cannot read the ingestion folder." | "Check that the folder exists, is mounted, and can be read by the account Reverie runs as." |
| Library folder unavailable | "Reverie cannot write to the library folder." | "Check that the library folder is mounted and can be written by the account Reverie runs as." |
| Library full | "The library folder has no free space left." | "Free some space on the library's disk." |
| Another instance | "Another Reverie server is already running ingestion for this library." | "Make sure only one Reverie server uses this database." |
| Not specified | "Reverie hit a problem it cannot describe here." | "Check the server log for the cause." |
| Paused by an administrator | Copy not yet settled. | Copy not yet settled. |

The closing sentences of the paused notice ("Reverie checks again every few minutes ...") also depend on the reason. The
wording for "Paused by an administrator" is settled together with its sentence and check.

### Other paused-notice states

- **Read failed.** One quiet line, not boxed: "Could not check whether ingestion is running." / "Files may still be
  importing." with an outline "Try again".
- **Forbidden (403).** An alert: "This account cannot see ingestion status" / "Sign in as an administrator."
- **Session ended (401).** "Your session has ended" / "Sign in again to see ingestion status."
- **While paused or stopped:**
  - The scan hint reads "Imports wait while ingestion is paused."
  - An accepted scan keeps its title and its detail reads "Ingestion is paused, so nothing is being imported yet. Queued
    and waiting files start by themselves once it resumes. Unchanged files were left as they are."
  - The latest-activity line is hidden, so a stale "running" batch cannot contradict the notice.
- **Refresh.** The status is read when the page opens and on window focus, and every 30 seconds while the page is
  visible.

### Retry all

- **Placement.** Each eligible group header carries an outline "Retry all" button beside the file or book count. It is a
  separate control next to the expand-and-collapse button, not inside it, and it is visible when the group is collapsed.
  Tab order is the header button, then "Retry all". It is never a primary button.
- **Eligible groups.** Among files, only Needs a change and Failed after several tries. Among books, every group. Not
  the rejected groups, the ignored group or the Duplicates card.
- **No confirmation.** One press queues the group. A retry is cheap and the label states the number.
- **Provider wait.** Bulk enrichment retry preserves the earliest retry time requested by the source. Queuing a group
  does not skip that wait.
- **Label.** "Retry all {n}", where {n} is the group's count. A group of one reads "Retry 1 file" or "Retry 1 book".
- **Pending.** The button reads "Queuing {n}" and is marked `aria-disabled`.
- **Feedback.** A line beneath the header, visible when the group is collapsed and kept when the group's count drops:
  - "Queued {n}" when the whole group was queued. The button then reads "Queued" and is inactive.
  - "Queued {n}, {m} remaining" when a limit on one press left some behind. The button then reads "Retry the rest {m}".
  - While ingestion is paused, file feedback adds "Ingestion is paused, so they will run once it resumes. They return
    here if they fail again."
- **Nothing to retry.** "Nothing to retry. This group changed after the page loaded."
- **Failure.** The alert "Could not queue these files. Nothing was changed." ("these books" for books). The header
  button reads "Try again".
- **Forbidden (403).** "This account cannot retry files. Sign in as an administrator." The button is disabled for the
  rest of the page session.
- **Session ended (401).** "Your session has ended. Sign in again to retry files."
- The group and its count are not refreshed at the moment of the press, so the feedback stays visible. They update on
  the next refresh.

### Duplicates card

A card of its own, between Needs attention and Enrichment problems. A duplicate is a deliberate skip, not a failure, so
it is not part of the Needs attention card or its total.

- **Head.** The heading "Skipped as duplicates" and, in fixed-width type, "{n} files" ("1 file").
- **Intro.** "Files that match a book already in the library, so Reverie did not import them again. Delete them if you
  do not need them. Removing duplicate source files under Settings, Acquisition applies only to duplicates found from
  then on."
- **Table.** Columns "File", "Matches" and "Last tried". File is the relative path, with the directory muted. Matches is
  the matching book's title, linking to the book's page. When the matching book no longer exists: "No matching book on
  record." Last tried is the time, as elsewhere.
- **Footer.** "Showing {n} of {total}", "Show more", "Loading more", behaving as for the other lists.
- **Loading.** A busy skeleton with the status text "Loading skipped duplicates".
- **Failed.** "Could not load this list." / "Your files are untouched. Try again." with "Try again".
- **Empty.** The card stays, with "No files were skipped as duplicates." / "If removal of duplicate source files is on
  under Settings, Acquisition, Reverie deletes a duplicate when it finds it, so it does not appear here."
- The card is read-only and has no "Retry all".

## Keyboard, focus and accessibility

- Tab order: the scan button, then each group header in order, with the controls inside an open group after its header
  (rows' "Try again", "Show more", footer "Try again").
- Group headers are real buttons. Enter and Space toggle them, and `aria-expanded` and `aria-controls` stay in step.
- The status region is always present and polite. Request failures and 401 and 403 answers are alerts.
- Skeletons are busy regions with a status string. Progress bars have a role, a name and values.
- Tags carry words ("Rejected", "Needs a change", "Ignored"). Nothing is carried by colour alone, and no danger colour
  appears on this page.
- After "Show more", focus stays on the button while more rows remain. After "Try again" on an enrichment row, focus
  stays in the row.
- One global focus ring applies to every control, in both themes.
- Under reduced motion the skeleton pulse, the spinner, the chevron turn and the progress fill transition stop.
- One primary button is on the page: "Scan ingestion folder". On the library page's empty state, "Review these files" is
  the only one.
- Copy uses Australian English, with no exclamation marks.

## Acceptance checklist

A rendered page is accepted when every item holds, in the dark theme and the light theme, at 1920 by 1080 or larger.
Items about the target design apply once those parts are built.

1. The page head, scan control, status region, cards and metric cards appear in the order given, in a column no wider
   than 1152 px.
2. Each scan state shows its exact title and detail, and no batch figure is labelled as a particular scan's result.
3. The Needs attention total equals the sum of the group counts and excludes the ignored group. A file with two reasons
   appears once, under its primary reason, with an "Also" line, and in no other group.
4. Groups appear in the specified order, the first attention group is expanded on load, and the ignored group is last,
   collapsed and muted.
5. Expanding a group requests that group's files by its own cursor. The count in the header matches the rows across all
   pages. Collapsed groups make no requests.
6. Paths are relative and shown with a muted directory. A file name that is not valid UTF-8 renders with its escapes.
7. Enrichment groups are headed by source and class, each book appears once, and "Try again" moves through "Queuing" to
   "Queued" without the row disappearing.
8. Refresh follows the rules above: every 3 seconds while a batch runs, 2 minutes after a scan that queued or deferred
   files, then stopping, with one owner of the timer.
9. Every loading, empty and failed state shows the copy given for it, with focus behaving as described.
10. The empty library shows the variant for its viewer and condition, in the stated precedence, and never shows an
    administrator a folder path. A non-administrator sees nothing about administration.
11. A keyboard-only pass reaches the scan button, every group header, "Show more", "Try again" and the links. No control
    is a clickable non-button, and one focus ring shows throughout.
12. Once built: the paused notice, "Retry all" and the Duplicates card each match their sections above, and a healthy
    page looks the same as before the notice existed.

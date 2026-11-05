---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0030"
title: "Accessible application controls"
satisfies:
  - "REV-REQ-0004"
governed-by:
  - "REV-ADR-0060"
---

# Accessible application controls

## Purpose and boundaries

The frontend's control composition supplies keyboard, pointer, focus and validation behaviour across application
screens. React Aria Components owns those interaction semantics. Local wrappers apply the visual specifications in
[frontend/DESIGN.md](../../../../frontend/DESIGN.md). Domain validation, API mutations, routing and grid editing remain
with their owning modules.

## Structure

Shared controls live in `frontend/src/components/ui/`. Buttons, selection controls, disclosures, tabs and overlays
compose React Aria primitives. Styled `Input`, `Label` and `Textarea` wrappers participate through the supported
`render` API; their styling does not replace the parent field's semantics. Static labels, separators and badges use
semantic HTML. React Router Links use `buttonVariants` when they need button styling.

Radix Colors and `src/styles/themes/primitives.generated.css` supply colour primitives independently of the interaction
library. Existing controls consume compatibility aliases bound to the canonical palette. The Radix UI runtime is not
installed. The grid engine owns its editing, sorting and loading lifecycle separately from shared controls.

## Interfaces and dependencies

React Aria exposes native state through `isSelected`, `isReadOnly`, `isDisabled` and `isPending`, with corresponding
data attributes for styling. Label and description associations belong to the parent field or overlay composition.
Buttons handle activation through `onPress`; navigation links retain link semantics and Router integration.

Grid cell renderers receive the engine's child tab index. Title links forward it, and details buttons translate it to
React Aria's `excludeFromTabOrder`, so keyboard entry returns to the active cell and arrow keys reach its controls.

Submit forms use React Aria `Form` and the React `action` prop. Actions await API mutations. Named fields supply
FormData, and domain validators parse that input before requests. The React Aria `validationErrors` contract associates
field errors and moves focus to the first invalid field on a blocked submission.

## Data and state

Uncontrolled submit fields take initial and returned values through `defaultValue`. `useActionState` carries submitted
values and expected errors across action completion, so the successful React action reset does not discard refused
input. Controlled values remain appropriate where edits drive other UI, including settings drafts and grid editors.

Create-user role and token expiry are named, uncontrolled Selects. Failed requests retain their choices; reopening a
closed dialog restores defaults. Settings owns drafts, dirty patches and concurrency handling. Its actions are disabled
when pristine, and Save remains in the tab order while pending.

Shelf creation starts a fresh form on each dialog visit. Dismissing a failed attempt discards its submitted name and
error; retrying within the open dialog retains both.

## Runtime behaviour

Persistent write initiators use native Button pending state: they retain focus and suppress repeated activation while
the request settles. Conflicting siblings are disabled. Batch actions use a labelled Group with ordinary Tab order and
immediate native shelf menus.

Role changes retain focus on the Select trigger through its native Button pending state. The Select disables its native
input while the write settles. The trigger overrides the inherited disabled prop and uses pending behaviour to stay in
the Tab order and suppress activation. Confirmed user data controls the selected role.

Actions stay pending until the awaited mutation and its completion work settle. Expected refusals retain entered values
and allow correction or retry. Successful submission preserves each screen's navigation, reset or dialog closure
behaviour. Token receipt values use read-only TextFields with associated Labels and styled Inputs.

Settings saves use the router's default transitions. Navigation requested during a save remains pending until the action
settles. A successful save proceeds through the blocked navigation; a failed save keeps the draft and presents the
leave-or-stay prompt.

Vocabulary pickers supply server suggestions to the ComboBox collection. ComboBoxValue renders hydrated selections
separately, in selected-key order rather than server rank. Multiple selection retains the native popup lifecycle.

The command palette uses MenuItem links with Router navigation and closure. `Autocomplete` keeps input focus during
arrow navigation; Tab traverses the result links. The Menu owns scrolling. Dialog and Select labels belong to their
native parent composition.

App owns palette open state and passes it through ordinary props. The shell DOM boundary uses `useKeyboard.shortcuts`
for Control+K and Meta+K, opening the palette through native matching and propagation. Portal and document-body focus
are outside that boundary. Native close resets the palette query. The existing Library SearchField keeps its own filter
behaviour and location.

The cinematic ToggleButton controls the existing presentation owner through `isSelected` and `onChange`. It remains
visible outside hidden chrome at the bottom centre; visibility removes hidden chrome from keyboard traversal. The
pointer-idle timer and reduced motion remain presentation concerns. Visible Help opens the native shortcuts dialog. Home
and Stats have static Planned status within their existing non-interactive rows.

Ingestion disclosures mount query children only while expanded. Book tabs activate from arrow keys and mount only the
selected panel. Filter disclosures retain their mounted editors, as described by the
[Library state Design](../../library/design/0001-library-filter-and-sort-state.md).

## Failure and recovery

Publication edits use one text field. Server `errors.pub_date` reaches `Form.validationErrors` through `useActionState`
and the native `FieldError` identifies the refused field. The refused date and other submitted edits remain available;
editing the field clears its native server error and permits correction without a competing client parser.

Feedback stays with the action's context. Inline errors are not repeated in a toast. Copy status stays beside the
read-only credential and retains manual-copy recovery. Row actions retain named failures beside their target. Membership
outcomes remain outside the closing menu. Drawer shelf additions capture the book and shelf in account/book-scoped
mutation variables. Reopening the same book observes its latest cached addition, retaining progress, result and retry
across drawer dismissal. These records use native mutation-cache retention; cache observation does not extend it. Batch
progress and consolidated results derive from a serial Query mutation's captured IDs and shelf; failed-ID retry and the
result remain available when selection becomes empty. Completion removes only succeeded IDs from the current selection.
The batch owner stays mounted across table, grid and blocked-result branches. Versions observes pending and failed
review mutations to retain their field groups when the last draft disappears from refreshed book data. Theme validation
refusals render below the User chip; cookie and outage policies stay with the preference owner. Auth success uses fixed,
allowlisted copy on the login destination.

Field-specific validation appears beside its field with native associations. General API failures remain standalone
alerts rather than being assigned to an unrelated field. Pending controls suppress duplicate submission while keeping
the entered values available when the request settles. Cancellation and successful completion retain the owning screen's
existing state transitions.

Metadata clear confirmations suppress repeated activation while the request is pending. Completion closes the
confirmation; a refused save leaves the sheet open with the entered values and error feedback. The form's submit event
opens the confirmation before the action runs, including for implicit submission. The confirmation button allows the
next submission through to the action.

Table-load recovery belongs to the Library results region and is described in the Library state Design. Cell failures
also render in that region, outside conditional table rendering, from account-scoped TanStack mutation state. Each
mutation captures its book, field, submitted value and displayed source query. Mutation lifecycle handlers apply
confirmed results even after table replacement; the existing synchronous grid adapter supplies pending-cell protection.
A successful save or Undo removes earlier failed mutations for the same book and field from the account's cache. A
failure arriving after a newer cached success for that cell is removed on delivery. Removed failures stay retired after
the correcting success is collected. Targeted metadata and reading-state queries re-read the affected resource
independently of list membership. They read on initial recovery, reconnect and explicit Reload, without another read on
window focus. Fresh reads update the captured source and reconcile matching failures without repeating writes or
creating Undo provenance. A matching read disables further recovery reads for that operation; its cached confirmation
stays resolved across later edits. Mutation-cache retention is in-memory, not a durable operation ledger.

Before a cell write starts, its mutation lifecycle cancels conflicting recovery queries through TanStack and their
transport AbortSignal. Recovery waits while a write affects the same resource; a work-scoped metadata write also
coordinates recovery for sibling editions. Reading writes and unrelated resources remain independent. Reload retains
native pending behaviour during that wait. The subscribed mutation snapshot supplies a literal disabled state while
blocked. After settlement, the native observer sees that transition and automatically resumes unresolved recovery.
Aborted reads cannot apply stale resource or list values or replace a newer response's ETag.

Failed mutations use TanStack's native retention: they remain while observed, then become eligible for collection five
minutes after their last mutation observer detaches. The initiating hook's next request or table removal detaches that
observer; `useMutationState` does not extend the lifetime. Collection removes the failure message, displayed submitted
value, Reload action and reconnect recovery. The interval starts at detachment, not when feedback appears. No separate
feedback timer, extended cache lifetime or Dismiss control applies.

The table owns a bounded, ten-entry sequential Undo history. Undo last edit and the table-scoped Control/Meta+Z shortcut
use the same latest valid entry; native text-input Undo remains available in editors. The visible button names its field
and book, retains focus while pending and waits for in-flight cell operations. Metadata Undo preserves the response's
previous-version pointer or required counter-patch; reading Undo applies its own server response. Failed Undo retains
its entry for correction or retry. Removing the table ends access to this history. Shared controls do not change the
grid's editing model. The application uses local feedback without a notification runtime.

## Security and operations

Browser validation provides feedback; API validation and authorisation remain server responsibilities. Form values stay
in the owning component and request flow. Authentication actions retain the existing CSRF and session contracts. No
control adds persistent credential storage.

Browser Mode tests exercise native focus, validation and pending behaviour with vitest-browser-react. Standalone
Chromium and Firefox journeys verify document navigation through the production application. Pure calculations run in
Node. These synthetic API fixtures do not establish server authentication or database behaviour. Test commands and
browser provisioning are documented in the [frontend README](../../../../frontend/README.md).

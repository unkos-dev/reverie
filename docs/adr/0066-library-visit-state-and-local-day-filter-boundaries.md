---
type: ADR
profile-version: 1
id: "REV-ADR-0066"
title: "Library visit state and local-day filter boundaries"
status: "accepted"
recorded-on: "2026-10-25"
decision-makers:
  - "John Unkovich"
---

# Library visit state and local-day filter boundaries

## Context and problem statement

Library filters describe a reader's current visit. Reload and returning from a book should recover that context, while a
fresh Library entry should start clean. Filter edits do not need shareable URLs or individual Back steps. Native numeric
and date controls also have their own unfinished input, which differs from the last value they publish.

An added-date filter describes local calendar days. A UTC calendar date cannot express those boundaries for every
reader, and adding 24 hours cannot express the next local midnight across daylight-saving changes. The browser knows the
reader's timezone; the API and its cursors need unambiguous instants.

## Decision drivers

- Recover a visit through reload and history traversal without turning each filter edit into navigation.
- Keep one owner of published intent and let supported native controls own unfinished input.
- Retain the shared query cache used by pagination, row edits, undo and conflict recovery.
- Include both selected local days across timezone offsets and daylight-saving changes.
- Preserve typed input rejection, bound SQL parameters and cursor identity.

## Considered options

- React visit state persisted on the current Navigation entry, with local-day boundaries projected into API instants.
- Browser query parameters with delayed writes and application-owned input drafts.
- A separate persistent client store for filters, with explicit visit identifiers and cleanup.
- Server-side timezone conversion from calendar dates and a client-supplied timezone.

## Decision outcome

Chosen option:
**React visit state persisted on the current Navigation entry, with local-day boundaries projected into API instants**,
because the entry supplies the required visit lifetime without per-edit navigation or an additional store, and the
browser can convert the reader's local days before the API evaluates timestamp predicates.

Native NumberFields retain snapping, steppers and intrinsic integer domains. Independently optional DatePickers retain
segments and calendars. Completed contradictory intervals describe an empty set; they do not require a separate
last-valid query. Native publication drives the current query immediately, including complete calendar-valid year
prefixes. Uniform drawer dismissal retains published edits without an Apply or Cancel transaction.

The API retains its flat suffix grammar, with inclusive `created_at_gte` and exclusive `created_at_lt` RFC 3339
instants. The upper day advances by one calendar day before timezone conversion. Equivalent offsets identify the same
instant for cursor fingerprints. The shared QueryClient retains previous pages while the current request updates;
displayed rows keep their source key for mutations and undo.

### Consequences

- Positive: reload and traversal recover a visit without filter-bearing links or edit-level history.
- Positive: native controls replace application drafts, correction and write-delay machinery.
- Positive: local-day boundaries remain correct across daylight-saving transitions without backend timezone data.
- Negative: Library visit persistence requires Navigation-capable recent browsers.
- Negative: native year-prefix publication can briefly query intermediate dates and show intermediate results.
- Negative: retained rows require explicit updating feedback and source-key provenance for cache actions.
- Negative: filters are not portable bookmarks or shared links; a restored old entry may retain an old visit.

## More information

- [Typed filter grammar](0038-typed-filter-grammar-on-list-endpoints.md) retains the API suffix convention. Its
  shared-link consequence does not describe the Library browser address.
- [Uniform dismissal](0062-uniform-dismissal-for-library-filter-edits.md) retains one dismissal outcome. Its delayed URL
  writes and application-owned numeric/date drafts do not describe the native visit model.
- [Sort preference](0047-library-sort-is-a-per-user-preference-never-url-state.md) retains the independent account
  preference and client-resolved sort contract.
- [Navigation entry updates](https://developer.mozilla.org/en-US/docs/Web/API/Navigation/updateCurrentEntry) describe
  state persistence without navigation.
- [CalendarDate](https://react-aria.adobe.com/internationalized/date/CalendarDate) documents calendar arithmetic and
  conversion to a date in a timezone.

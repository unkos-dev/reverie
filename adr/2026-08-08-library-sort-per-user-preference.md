---
status: "accepted"
date: 2026-08-08
supersedes: []
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# Library sort is a per-user preference, resolved client-side, never URL state

## Context and Problem Statement

The library's sort stack originally lived in the URL (`?sort=`), a placement
the [multi-column sort stack ADR](2026-07-07-multi-column-sort-stack.md)
carried as a decision driver: a URL sort is bookmarkable and shareable. When
per-user display preferences moved to the server (`user_preferences`,
`/auth/me/preferences`), sort persistence was first designed as a second
layer under that URL state: the URL carried the "current" sort, the account
carried a "default" that applied only when the URL was silent.

Review of the implementation found that the two-layer model was the direct
parent of every material defect on the sort path. The layers gave the same
conceptual value two readers with different resolutions, so surfaces
disagreed about the active sort. Absence of `?sort=` was ambiguous between
"inherit my default" and "sorting is off", which made an inherited descending
default impossible to clear from the sort controls: the clearing gesture
wrote an absent parameter over an already absent parameter, and the default
reapplied on the next render. The route loader could only see the URL, so any
account default guaranteed a cache-key mismatch and a discarded prefetch. The
URL grammar had two states for three claimed meanings (an explicit sort,
inherit, explicitly none).

Where should the library's sort stack live, who resolves it, and what may the
sort controls promise, so that one reader's preferred order follows them
between devices without a second source of truth?

## Decision Drivers

- **One reading.** Every surface that renders or edits the sort (table
  headers, the rail's stack editor, the live region, the list request) must
  see the same resolved value, or a read split recreates stuck states.
- **Lifetime honesty.** The state-ownership rule keeps library filters in the
  URL because they must survive a refresh but not a fresh visit days later.
  Sort has the opposite requirement: it must survive reload, re-login, and a
  different device. A medium chosen for one lifetime should not carry the
  other.
- **Keyset integrity.** The books cursor embeds the canonical sort spec and
  rejects a continuation minted under a different stack. Whatever resolves
  the sort must keep every page of one query under one explicit spec.
- **Loader compatibility.** The route loader prefetches the first page; the
  component must ask for the key the loader seeded, or every navigation pays
  a discarded request.
- **Deployment shape.** This is a self-hosted household instance: no
  anonymous traffic, and links are not a distribution channel. URL state
  earns its costs only where sharing or statelessness is the point, and
  neither applies to a reader's own ordering.

## Considered Options

- **A: Two layers. URL carries the current sort; the account carries a
  default applied when the URL is silent.**
- **B: One layer, client-resolved. Sort is a per-user preference; gestures
  write the preference; the client sends the resolved sort explicitly on
  every list request; the URL never carries sort.**
- **C: One layer, server-resolved. The list request omits sort; the backend
  resolves the caller's stored preference, falling back to the installation
  default.**
- **D: Device-local persistence only (localStorage), no server tier.**

## Decision Outcome

Chosen option: **B**.

The decision:

- **Sort is a per-user preference with a single source of truth.** The
  stored `sort_stack` is the reader's override; `null` means inherit the
  installation default. Every sort gesture writes the preference directly
  through one intent handler. Nothing infers intent from URL diffs.
- **The client resolves and sends the sort explicitly.** The effective sort
  is resolved once per page (override, else installation default) and every
  consumer reads that one resolution. The list request carries the override
  explicitly when one exists and omits the parameter when inheriting, so an
  inheriting reader's query key equals the loader's URL-derived seed key, and
  the installation default is never serialized into a request it would not
  change. The wire contract of the multi-column sort ADR is unchanged.
- **The URL never carries sort.** The `?sort=` parameter is retired from the
  library surface; a stale parameter in an old bookmark is inert, consistent
  with how the filter codec already treats dead parameters.
- **There is no unsorted state.** A keyset-paginated library always has a
  total order, so the controls stop promising otherwise. Header clicks
  toggle ascending and descending only. The stack editor allows add, remove,
  reorder, and flip; removing the last level and the explicit reset both
  write `null` and visibly transition to the installation stack. The
  effective order is displayed truthfully everywhere, including when it is
  the inherited default.

### Consequences

- Good, because every prior defect class is structurally unrepresentable:
  no read split, no unclearable state, no loader key mismatch for inheriting
  readers, no absent-versus-off ambiguity.
- Good, because a changed installation default reaches every reader who has
  not overridden sort, which a materialize-on-write store cannot do.
- Bad, because a link can no longer carry a sort and the back button no
  longer steps through sort states. Link sharing is an explicit non-goal of
  this deployment shape, and stepping history through orderings has no
  meaning for a durable personal preference.
- Bad, because a reader with a sort override pays one visible correction on
  a device whose local mirror is missing or stale: the first page renders in
  the seeded order, then re-sorts when the preference arrives. The
  first-paint contract already accepts this for every preference group.
- Neutral, because every exploratory sort becomes the durable preference,
  matching how the view toggle already behaves: the last choice is the
  standing choice.

### Confirmation

The library surface has no `?sort=` reader or writer; lint and tests keep
`useSearchParams` off the surface, and the filter hook owns no sort key. One
intent handler serializes every sort gesture into the preference write, and
regression tests pin the case the two-layer model failed: clearing an
inherited descending default from each sort surface must reach the wire as
`sort_stack: null`. The list request never carries the installation default.

## Pros and Cons of the Options

### A: Two layers, URL current over account default

- Good, because links and history carry the sort, and the URL machinery
  needs no change.
- Bad, because two readers resolving one value differently produced real
  defects: surfaces disagreed, an inherited descending default could not be
  cleared, and the loader's prefetch missed for anyone with a default.
- Bad, because absence in the URL cannot distinguish inherit from off, and
  the grammar cannot be taught the difference without a sentinel value.

### B: One layer, client-resolved, explicit on the wire

- Good, because one resolution feeds every consumer and the query key is
  deterministic from client state the loader can also read.
- Good, because the sort gesture and the persistence write are independent:
  a failed preference write costs durability, never the visible re-sort.
- Bad, because the loader needs the local mirror to seed the right key, and
  a missing or stale mirror costs one visible correction.

### C: One layer, server-resolved on the list endpoint

- Good, because no client resolution exists at all and a cold load needs no
  preference knowledge.
- Bad, because the books cursor rejects a continuation whose sort differs
  from the request's: with the sort implicit, a preference change between
  pages turns benign cross-tab drift into a failed Load more.
- Bad, because every sort gesture becomes a serialized write-then-refetch,
  and a failed write makes the gesture itself visibly do nothing.
- Bad, because the catalog's core list endpoint becomes nondeterministic per
  caller: the same request from the same caller returns different orderings
  as hidden state changes, and every future consumer must know that omitting
  sort means "caller's preference" rather than "canonical order".

### D: Device-local persistence only

- Good, because it is the least machinery.
- Bad, because sort then fails the requirement that display preferences
  follow the reader between devices, which is the reason the server tier
  exists.

## More Information

- [Multi-column sort stack](2026-07-07-multi-column-sort-stack.md): the wire
  grammar, whitelist, and cursor contract this decision leaves untouched. Its
  "sort lives in the URL" driver is revised by this ADR; the rest stands.
- [Persisted settings](2026-05-26-persisted-settings.md): the settings tier
  model behind the `user_preferences` row this decision writes to.
- Revisit trigger: if named saved views land, they may want to apply a sort
  without changing the reader's standing preference; that distinction needs
  its own representation and belongs to that design, not this one. If link
  sharing ever becomes a real workflow, revisit the URL stance for sort
  together with filters rather than alone.

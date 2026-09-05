---
type: ADR
profile-version: 1
id: "REV-ADR-0047"
title: "Library sort is a per-user preference, never URL state"
status: "accepted"
recorded-on: "2026-09-06"
decided-on: "2026-08-08"
decision-makers:
  - "John Unkovich"
---

# Library sort is a per-user preference, never URL state

## Context and problem statement

The library's sort stack originally lived in the URL (`?sort=`), a placement the
[multi-column sort stack ADR](./0037-multi-column-sort-stack-on-the-keyset-list-contract.md) carried as a decision
driver: a URL sort is bookmarkable and shareable. When per-user display preferences moved to the server
(`user_preferences`, `/auth/me/preferences`), sort persistence was first designed as a second layer under that URL
state: the URL carried the "current" sort, the account carried a "default" that applied only when the URL was
silent.

Review of the implementation found that the two-layer model was the direct parent of every material defect on the
sort path. The layers gave the same conceptual value two readers with different resolutions, so surfaces disagreed
about the active sort. Absence of `?sort=` was ambiguous between "inherit my default" and "sorting is off", which
made an inherited descending default impossible to clear from the sort controls: the clearing gesture wrote an
absent parameter over an already absent parameter, and the default reapplied on the next render. The route loader
could only see the URL, so any account default guaranteed a cache-key mismatch and a discarded prefetch. The URL
grammar had two states for three claimed meanings (an explicit sort, inherit, explicitly none).

Where should the library's sort stack live, who resolves it, and what may the sort controls promise, so that one
reader's preferred order follows them between devices without a second source of truth?

## Decision drivers

- One reading: every surface that renders or edits the sort (table headers, the rail's stack editor, the live
  region, the list request) must see the same resolved value, or a read split recreates stuck states.
- Lifetime honesty: the state-ownership rule keeps library filters in the URL because they must survive a refresh
  but not a fresh visit days later. Sort has the opposite requirement: it must survive reload, re-login, and a
  different device. A medium chosen for one lifetime should not carry the other.
- Keyset integrity: the books cursor embeds the canonical sort spec and rejects a continuation minted under a
  different stack. Whatever resolves the sort must keep every page of one query under one explicit spec.
- Loader compatibility: the route loader prefetches the first page; the component must ask for the key the loader
  seeded, or every navigation pays a discarded request.
- Deployment shape: this is a self-hosted household instance, with no anonymous traffic, and links are not a
  distribution channel. URL state earns its costs only where sharing or statelessness is the point, and neither
  applies to a reader's own ordering.

## Considered options

- **Two layers**: URL carries the current sort; the account carries a default applied when the URL is silent.
- **One layer, client-resolved**: sort is a per-user preference; gestures write the preference; the client sends
  the resolved sort explicitly on every list request; the URL never carries sort.
- **One layer, server-resolved**: the list request omits sort; the backend resolves the caller's stored preference,
  falling back to the installation default.
- **Device-local persistence only**: persistence lives in `localStorage`, with no server tier.

## Decision outcome

Chosen option: **One layer, client-resolved**, because every prior defect traced to a read split between two
sources of the same value, and this option gives every surface a single resolution while keeping the wire contract
of the multi-column sort ADR unchanged.

The decision:

- Sort is a per-user preference with a single source of truth. The stored `sort_stack` is the reader's override;
  `null` means inherit the installation default. Every sort gesture writes the preference directly through one
  intent handler. Nothing infers intent from URL diffs.
- The client resolves and sends the sort explicitly. The effective sort is resolved once per page (override, else
  installation default) and every consumer reads that one resolution. The list request carries the override
  explicitly when one exists and omits the parameter when inheriting, so an inheriting reader's query key equals
  the loader's URL-derived seed key, and the installation default is never serialized into a request it would not
  change.
- The URL never carries sort. The `?sort=` parameter is retired from the library surface; a stale parameter in an
  old bookmark is inert, consistent with how the filter codec already treats dead parameters.
- There is no unsorted state. A keyset-paginated library always has a total order, so the controls stop promising
  otherwise. Header clicks toggle ascending and descending only. The stack editor allows add, remove, reorder, and
  flip; removing the last level and the explicit reset both write `null` and visibly transition to the
  installation stack. The effective order is displayed truthfully everywhere, including when it is the inherited
  default.

### Consequences

- Positive: every prior defect class is structurally unrepresentable: no read split, no unclearable state, no
  loader key mismatch for inheriting readers, no absent-versus-off ambiguity.
- Positive: a changed installation default reaches every reader who has not overridden sort, which a
  materialize-on-write store cannot do.
- Negative: a link can no longer carry a sort and the back button no longer steps through sort states. Link sharing
  is an explicit non-goal of this deployment shape, and stepping history through orderings has no meaning for a
  durable personal preference.
- Negative: a reader with a sort override pays one visible correction on a device whose local mirror is missing or
  stale: the first page renders in the seeded order, then re-sorts when the preference arrives. The first-paint
  contract already accepts this for every preference group.
- Negative: every exploratory sort becomes the durable preference, matching how the view toggle already behaves:
  the last choice is the standing choice.

## Pros and cons of the options

### Two layers

- Positive: links and history carry the sort, and the URL machinery needs no change.
- Negative: two readers resolving one value differently produced real defects: surfaces disagreed, an inherited
  descending default could not be cleared, and the loader's prefetch missed for anyone with a default.
- Negative: absence in the URL cannot distinguish inherit from off, and the grammar cannot be taught the difference
  without a sentinel value.

### One layer, client-resolved

- Positive: one resolution feeds every consumer and the query key is deterministic from client state the loader
  can also read.
- Positive: the sort gesture and the persistence write are independent: a failed preference write costs
  durability, never the visible re-sort.
- Negative: the loader needs the local mirror to seed the right key, and a missing or stale mirror costs one
  visible correction.

### One layer, server-resolved

- Positive: no client resolution exists at all and a cold load needs no preference knowledge.
- Negative: the books cursor rejects a continuation whose sort differs from the request's: with the sort implicit,
  a preference change between pages turns benign cross-tab drift into a failed Load more.
- Negative: every sort gesture becomes a serialized write-then-refetch, and a failed write makes the gesture itself
  visibly do nothing.
- Negative: the catalog's core list endpoint becomes nondeterministic per caller: the same request from the same
  caller returns different orderings as hidden state changes, and every future consumer must know that omitting
  sort means "caller's preference" rather than "canonical order".

### Device-local persistence only

- Positive: it is the least machinery.
- Negative: sort then fails the requirement that display preferences follow the reader between devices, which is
  the reason the server tier exists.

## More information

- [Multi-column sort stack](./0037-multi-column-sort-stack-on-the-keyset-list-contract.md): the wire grammar,
  whitelist, and cursor contract this decision leaves untouched. Its "sort lives in the URL" driver is revised by
  this ADR; the rest stands.
- [Persisted settings](./0012-persist-operator-tunable-settings-to-database-with-live-reload.md): the settings tier
  model behind the `user_preferences` row this decision writes to.
- Revisit trigger: if named saved views land, they may want to apply a sort without changing the reader's standing
  preference; that distinction needs its own representation and belongs to that design, not this one. If link
  sharing ever becomes a real workflow, revisit the URL stance for sort together with filters rather than alone.

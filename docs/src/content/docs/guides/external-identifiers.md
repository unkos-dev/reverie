---
title: External identifiers and ratings
description: How Reverie stores provider identifiers, enriches by native id, caches per-provider ratings, and hides providers from display.
---

Reverie can identify a book on external catalogues even when the file carries
no ISBN. Each work and each edition can hold one identifier per provider
scheme, and the enrichment pipeline can resolve a book directly by those
native ids. Alongside the identifiers, Reverie caches each provider's
aggregate rating per edition.

## The identifier registry

Identifiers live at two levels, because a provider id names either the
abstract work or one specific edition:

| Level           | Meaning                                 | Example                            |
| --------------- | --------------------------------------- | ---------------------------------- |
| `work`          | Shared across every edition of the work | Open Library work `OL45804W`       |
| `manifestation` | Names one edition (one file)            | Google Books volume `zyTZAAAAYAAJ` |

Supported schemes: `openlibrary` (work and edition ids), `googlebooks`
(volume ids), `hardcover` (book ids or slugs), `goodreads`, `librarything`,
`wikidata`, `asin`, `oclc`, `lccn`, and `calibre`. Each `(book, scheme)`
pair holds a single value; setting a new id replaces the old one.

ISBNs are not part of the registry. They remain first-class columns on the
edition, where matching, search, and file writeback already rely on them.

### Editing identifiers

Identifiers are set and cleared through the metadata PATCH endpoint using
two maps, one per level:

```json
PATCH /api/v1/books/{id}/metadata
{
  "work_identifiers": { "openlibrary": "OL45804W" },
  "manifestation_identifiers": { "googlebooks": "zyTZAAAAYAAJ", "asin": null }
}
```

A `null` entry clears that scheme's slot. Every value is validated against
its scheme's format before it is accepted (for example, an Open Library
edition id `OL...M` is rejected in the work-level map). Edits are journaled
like any other metadata change, so they appear on the Versions tab and can
be reverted. An identifier edit also re-queues the book for enrichment
immediately, clearing any retry backoff from earlier failed runs. If an
enrichment run is active at the moment of the edit, the re-queue waits for
that run to finish instead of starting a second one; the book then re-enters
the queue on its own, since the active run looked the book up before the
edit landed.

Identifier changes never trigger a file writeback: they describe the book's
place in external catalogues, not the file's own metadata.

## Enrichment by native id

A book with no usable ISBN is enriched through its stored identifiers for
the three API-capable providers: Open Library, Google Books, and
Hardcover. Ids are tried in provider-priority order, most specific first
(edition ids before work ids), with each id routed only to its own
provider. A miss falls through to the next id, and title/author
search remains the last resort.

Enrichment also records the identifiers providers report, for example the
Google volume id of a matched edition or the Open Library work behind an
edition. An empty identifier slot fills automatically; a slot that already
holds a different value is never overwritten silently, the new observation
is staged for review instead. Providers without a usable public API
(Goodreads, Amazon, and the other manual-only schemes) are stored and
displayed but never contacted.

## Per-provider ratings

Providers that report an aggregate rating (Google Books, Hardcover, and
Open Library search results) have that rating cached per edition, keyed by
provider. Each provider is authoritative for its own scale, so Reverie
stores the score, the scale, and the review count as reported and computes
no cross-provider average. Ratings refresh in place on every enrichment
run; they are not journaled, cannot be locked, and are never written back
to the file.

A refreshed record that no longer reports a rating, or reports one Reverie
cannot store (a score outside the provider's own scale, an impossible
review count), clears the cached value rather than leaving an obsolete
score on display. A failed fetch, or a lookup path that carries no rating
data either way, leaves the cached value alone, so a provider outage does
not blank the ratings already on your shelf.

## Hiding providers from display

The admin settings carry a `provider_visibility` map that hides individual
providers from the library list and book detail responses:

```json
PUT /api/v1/settings
{ "provider_visibility": { "googlebooks": false, "asin": false } }
```

Hiding a provider hides both its identifiers and its rating wherever the
two share a key. Amazon is the exception by construction: its identifiers
use the `asin` key and its rating uses `amazon`, so the two surfaces are
toggled independently. Visibility is display-only; a hidden provider is
still stored and still used by enrichment. Changes apply immediately, with
no restart.

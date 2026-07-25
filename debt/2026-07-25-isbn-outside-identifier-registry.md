---
severity: low
surfaces: [contributor]
adopted: 2026-07-25
adopted-because: "the external-identifier registry shipped additive-only; ISBNs stayed in the manifestations columns because rematch, the partial value indexes, OPF writeback, and search are all wired to them, and relocating a load-bearing column for shape purity was all regression surface and no functional gain"
lift-when-class: internal-refactor
lift-when: a feature needs a uniform identifier read or write model across all schemes (for example scheme-generic dedup or a single identifier editing surface); at that point fold manifestations.isbn_10/isbn_13 into the registry as an isbn scheme and rewire rematch, the value indexes, writeback, and search to read it there
---

# ISBNs live beside the identifier registry, not in it

## Constraint

The external-identifier registry (`work_external_identifiers`,
`manifestation_external_identifiers`) is the normalized home for
provider identifiers, one value per `(entity, scheme)`. ISBNs predate
it and stay as `manifestations.isbn_10` / `isbn_13` columns: the
work-rematch flow, the partial value indexes, OPF writeback, and
search all read those columns directly.

## Workaround

Identifier-shaped data is split across two shapes. Code that wants
"every identifier for this book" must read the registry and the ISBN
columns separately, and ISBN-specific behaviour (rematch on change,
writeback) is wired to the columns rather than expressed per scheme.
The enrichment lookup order handles this explicitly by deriving the
ISBN key from the columns and the native-id keys from the registry.

## Lift trigger

Once a uniform identifier model is required, migrate the ISBN columns
into the registry as an `isbn` scheme, rewire rematch, the indexes,
writeback, and search onto the registry, and drop the columns. Until
then the split is cheap to carry and the registry stays additive.

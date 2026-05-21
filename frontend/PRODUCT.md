# Product

## Register

product

## Users

Reverie is for two overlapping personas operating the same self-hosted instance:

- **The operator** — an English-language fiction reader who self-hosts their personal ebook library, sourcing books from purchases, independent publishers, web archives, and the wild. They value data sovereignty, metadata quality, and a calm, considered surface that matches how they treat their books: as a curated, permanent record.
- **The dependent reader** — family members (including children) who share the operator's instance but only browse and read. The UI must communicate role and permission clearly without surfacing administrative scaffolding to them.

Both personas use Reverie across desktop (primary) and mobile (secondary). The operator's context is evening browsing — 5–15 minute mood-driven sessions, choosing the next book, occasional bulk metadata work — and longer admin sessions for ingestion and curation. Dependent readers' context is "find something to read now."

Single-tenant homelab installs and multi-user exposed instances are both first-class. The threat model and the visual model both treat the product as exposed.

## Product Purpose

Reverie is a self-hosted ebook library manager. Its job is to take a collection of EPUBs sourced from the wild, treat each one as a deliberate addition to a permanent record, surface the result as a library the operator is proud to browse, and get out of the way when they're reading.

Success looks like:

- An operator who keeps reverie running for years because each book they add feels like an act of stewardship, not data entry.
- A dependent reader who finds something to read in under a minute without confronting administrative surfaces.
- Metadata that is consistently better than what shipped inside the file — because the system treats Dublin Core as hypothesis and Reverie as the canonical record.
- A reading surface that disappears into the text, then a library surface that dignifies the catalogue when the operator comes back to it.

## Brand Personality

Editorial, weighted, permanent — with a cinematic-boutique register on the collector's-archive surfaces.

- **Editorial** — typography is opinionated and disciplined; Author for display moments, Satoshi for the workhorse, JetBrains Mono for metadata. The voice is restrained, deliberate, British-spelled. No breezy SaaS copy. No exclamation marks. No trailing em dashes.
- **Weighted** — the mark is a stone tablet with an inscription cut into it. Every chrome element should feel grounded, considered, like it could not be moved by accident. The opposite of frictionless.
- **Permanent** — inscription, not refresh. Adding a book is committing it to the record. The interface reinforces this: undo where feasible, typed-name confirmation where not, no clever delete affordances.
- **Cinematic, boutique** — the library and detail surfaces carry motion as a register signal (slow ambient drift in localised heroes, parallax on cover backdrops, 180–320 ms ease-out interactions). The reader withdraws. The cinematic half makes the archive feel alive; the boutique half keeps it from drifting into arcade.

## Anti-references

Surfaces that share the visual vocabulary of any of these are wrong, regardless of how well they're executed:

- **SaaS-cream AI-workflow chrome** (Linear, Stripe, vercel-cream marketing). The first-order category trap for self-hosted media managers in 2026. Reverie is not a workflow tool.
- **Generic ebook UI** (Calibre, Plex-clone shelves, default-Tailwind grids of identical cards with title + author + cover). Utilitarian CRUD is the category cliché — Reverie is in the category but must not look like it shipped from the category.
- **Cozy reading nook** (lamp, paper, warm-domestic, hand-drawn book stacks). The philosophy spec rejected this lane outright. The library is an archive, not a corner of a living room. The reader recedes; it does not invite.
- **Severity-coloured dashboards** (red/amber/green pills, info-blue callouts, status banners, "did you know" patterns). Gold means "this matters", Reverie Alarm means "stop", and everything else is weight, density, and motion. No third hue earns its screen.

Drift-checks: if a designer can guess theme + palette from category alone, the first reflex hasn't been avoided. If a designer can guess aesthetic family from category + anti-refs ("self-hosted media manager that's not Calibre → editorial-typographic"), the second reflex hasn't been avoided either. Both must be non-obvious.

## Design Principles

1. **Inscription, not decoration.** Every interface gesture is a commitment to the record. Add, edit, delete, enrich — each is an act of cataloguing. Surfaces should feel like the user is making something permanent, not refreshing a feed.

2. **One accent, one alarm, everything else is weight.** Reverie Gold says "this matters"; Reverie Alarm says "stop." State, hierarchy, and emphasis below that level are communicated through weight, opacity, type scale, density, and motion — never through additional hues. There is no severity ladder.

3. **The library does identity work; the reader recedes.** The cinematic-boutique register lives in the collector's-archive surfaces (home, library grid, book detail). The reader inherits palette and motion language but its chrome is the most minimal of all views. Two rooms in one house, both serving their primary task.

4. **Motion is felt, not flaunted.** No canvas-wide ambient drift. Localised hero treatments only. 180–320 ms ease-out (quart/quint/expo), never bounce, elastic, or spring. `prefers-reduced-motion` is respected as a first-class state — the cinematic register lives in palette and typography when motion recedes.

5. **Self-hosting is a posture, not a feature.** Treat users as operators with taste, not as customers of a hosted service. No upsells, no telemetry prompts, no trial-CTA scaffolding, no marketing growth-hook patterns. The product is for people who chose to own their library; the surfaces must respect that choice.

## Accessibility & Inclusion

Target: **WCAG 2.2 Level AA** as a design invariant, not a post-hoc check.

- All non-text UI components meet 1.4.11 (3:1 against adjacent surface). Body text meets 1.4.3 (4.5:1).
- The brand carries a deliberate, accepted constraint: Reverie Gold on Parchment (light theme accent) passes 1.4.11 and 1.4.3 large-text only — not 1.4.3 normal text. The gold is therefore restricted on light surfaces to focus rings, large CTAs, and recovery actions. axe-core contrast violations on small-text gold are the right signal: the surface is misusing the accent.
- Reverie Alarm is the only alarm hue and is fill-only — never body text. Both alarm values sit at hue 354°, deliberately cool of pure red so they cannot be misread as warm cousins of gold. Lightness is mode-bound; hue is the tunable for distinguishability.
- Colour-blind safety is engineered in: alarm is reinforcement, never the primary channel. Destructive intent and unrecoverable error are communicated through copy, friction, and iconography first; colour amplifies for users who can perceive it. The design must work with the colour stripped.
- `prefers-reduced-motion: reduce` is respected on every motion-bearing surface. Status-dot pulses become static, hover lift reduces to opacity change, cursor parallax disables. The cinematic register survives in palette and typography when motion withdraws.
- Keyboard navigation, focus visibility, and screen-reader semantics are first-class. Focus rings use the accent — that's part of the accent's "this matters" job.

Motion is welcome — within the budget above. Reduced-motion respect is an accommodation for the users who need it, not a default that flattens the surfaces for everyone.

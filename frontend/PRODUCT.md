# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

One self-hosted instance serves a household. Its users differ in how much of the library they want to manage:

- The metadata perfectionist curates and enriches the catalogue, and wants every field in the state they chose.
- The reader wants their books available on each device they read on, without managing the catalogue.
- The child is starting to read and sees a collection their parent approved.

Accounts are admin, adult or child. The person who deploys and administers the instance is an operator, and may be any
of the adult users above.

The parent decides which books their child can access and is the authority on what is appropriate for that child.
Arbitrary ratings do not determine suitability.

## Product Purpose

Reverie is a self-hosted ebook library manager. It gives a household the tools to curate and enrich a personal or family
ebook library, and makes that library available on the devices its users read on.

Metadata enrichment and curation are the foundation of the product.

Intended outcomes, one for each user:

- The metadata perfectionist can bring every field of every book to the state they want, and it stays that way.
- The reader finds their books on each of their reading devices. Reading progress shared between devices is planned.
- The child finds something to read among the books a parent approved, and meets nothing else.

## Positioning

Reverie has a narrow remit. It does not try to manage every kind of media. It aims to give the best experience for
curating a personal or family ebook library.

These mechanisms support that aim:

- A book is a work, not a file. Editions and formats of the same title group under one catalogue entry.
- Automated enrichment fills a field only when the field is empty, unlocked and in the set the system fills
  automatically. A change to an existing value is staged for review, with its source recorded, until a person accepts or
  rejects it. Automated enrichment never sets content rating.
- Ingestion copies each file from the ingestion directory into the managed library without altering its contents. By
  default it then removes the imported file from the ingestion directory. Removing a recognised duplicate is optional
  and off by default. A rejected or not-accepted file stays where it is.
- A child account sees nothing by default. A parent grants access for each shelf.

## Operating Context

Detailed curation is desktop-first. Mobile covers browsing, reading access and light edits.

Users read on their own devices. A web reader is intended as a fallback alongside those devices.

Single-household installs and internet-exposed multi-user instances are both first-class. The threat model treats every
instance as exposed.

## Capabilities and Constraints

- Reverie manages ebooks, with EPUB as the first format. Comics, graphic novels and PDF technical documentation are
  outside its remit. Audiobooks are a distant possibility and are undecided.
- OPDS is the current way for a reading device to reach the library. Sync with Kobo devices is planned. Reading progress
  shared between devices is planned. A dedicated Reverie app is possible and undecided.
- Reverie deploys with Docker. The backend is Rust, the frontend is React and TypeScript, and the database is
  PostgreSQL.
- The frontend builds on its chosen third-party tooling, including React Aria Components. Maintainability matters, so
  where that tooling solves a problem natively, the default is to use it instead of a hand-rolled solution.
- Reverie is under active development and has no published release.

## Brand Commitments

- The name is Reverie. It is the only brand commitment.
- Logo, wordmark, colour, type and motion are open. The brand files in the repository do not bind future design work.
- Interface copy uses Australian English. Localisation is a possible future consideration and is not in current scope.

## Evidence on Hand

No usage evidence is confirmed in this record. The outcomes above are intended and have not been validated. Reverie has
no users, testimonials, benchmarks or release history to cite, and future work must not invent them.

## Product Principles

1. Treat users as self-hosting operators. Reverie has no upsells, telemetry prompts or growth-hook patterns.
2. Treat metadata enrichment and curation as the product foundation. A value a person set outranks any automated source.
3. Keep decisions about a child's access to books with their parent. See the
   [access requirements](../README.md#design).
4. Keep the remit narrow. Reverie serves ebook curation well before it widens to other media.

## Accessibility & Inclusion

[REV-REQ-0004](../docs/specs/accessibility/requirements/0004-web-surfaces-meet-wcag-2-2-level-aa.md) is the required
standard. Every web surface Reverie ships conforms to WCAG 2.2 Level AA, except where that Requirement's approved
exceptions register covers a success criterion on a surface.

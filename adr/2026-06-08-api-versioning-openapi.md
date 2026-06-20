---
status: "accepted"
date: 2026-06-08
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# API versioning via URL path and OpenAPI 3.1 as the generated API contract

## Context and Problem Statement

The [JSON API conventions ADR](2026-05-22-json-api-conventions.md) fixed the
_shapes_ of Reverie's REST surface (field naming, RFC 7807 errors, cursor
pagination, merge-patch, content negotiation) but left two things unrecorded:

1. **The API has no version identifier.** Handlers mount flat under `/api/*`
   (`/api/books`, `/api/works/{id}`, `/api/shelves`). There is no `/v1` segment
   and no other version signal.
2. **There is no machine-readable API description.** No OpenAPI document exists
   and no spec tooling is in the backend dependencies, so an API reference would
   be hand-maintained and would drift from the handlers.

Both matter beyond the bundled UI. The frontend ships in the same image and is
versioned lockstep with the backend
([single-image distribution ADR](2026-05-05-single-image-distribution-central-csp.md)),
so it never needs a version negotiation, but device tokens make _third-party_
API access real (scripts, e-reader companions, automation), and those consumers
need both a stable version contract and a spec to generate against. Pre-release,
with no stable external clients yet, is the cheapest moment to stamp a version
and adopt a description format; both are breaking or expensive to retrofit once
external callers exist.

How should the API be versioned, and in what format is its contract described?

## Decision Drivers

- **Zero-drift reference docs**: the docs-as-done mandate requires the API
  reference to be generated, not hand-written, so it cannot fall behind the code.
- **Third-party clients**: device-token callers need a discoverable version and
  a spec they can codegen from.
- **Retrofit cost**: adding a version segment or a spec format after external
  clients depend on the surface is a breaking change; doing it greenfield is free.
- **Standards-default**: prefer an IETF/OpenAPI-blessed shape over bespoke
  invention, per the project's industry-standard-default principle.

## Considered Options

**Versioning scheme:**

- **V1: URL path major version (`/api/v1/...`).**
- **V2: Header / `Accept`-based version negotiation.**
- **V3: No versioning; evolve `/api/*` in place forever.**

**Contract format:**

- **S1**: OpenAPI 3.1, generated code-first from the handlers as the single
  source of truth.\*\*
- **S2**: A hand-written OpenAPI document maintained alongside the code.\*\*
- **S3**: No machine-readable spec; rustdoc / prose reference only.\*\*

## Decision Outcome

Chosen options: **V1 + S1.**

- **The JSON data API is served under `/api/v1`.** The major version lives in the
  URL path: discoverable, proxy- and cache-friendly, and free of
  content-negotiation machinery. Backward-compatible (additive) changes evolve
  `v1` in place; a future incompatible generation mounts as `/api/v2` _alongside_
  `v1` rather than breaking it. Pre-1.0 the contract may still tighten within
  `v1` under the project's `0.x` instability allowance, but the path version is
  the unit in which breaking _generations_ are expressed. This shifts the mount
  prefix the [JSON API conventions ADR](2026-05-22-json-api-conventions.md)
  assumed (`/api/books` → `/api/v1/books`) without changing any shape it fixed.
- **Operational and standard-protocol paths stay unversioned.** `/health`,
  `/readiness`, the `/auth` flow, and the `/opds` feed are not part of the
  versioned data API: liveness and auth are operational, and OPDS is versioned by
  its own specification.
- **The API contract is an OpenAPI 3.1 document generated code-first from the
  handlers**, as the single source of truth. It feeds the generated API reference
  and the CI docs gate (docs-as-done). A hand-written spec is rejected: it is a
  second source of truth that drifts from the code: exactly what the generated
  reference exists to prevent.

  The version is **3.1, not 3.2**, on three grounds. The code-first generator
  this would use (utoipa, the dominant axum-native option) emits 3.1, its
  `OpenApiVersion` enum has a single `3.1.0` variant, so pinning the contract to
  3.2 would pin it to an unreleased upstream capability, the same wait-on-upstream
  trap the
  [first-party session layer ADR](2026-06-04-first-party-session-layer.md)
  refused. 3.2's additions (querystring object schemas, Server-Sent Events
  metadata, JSON Lines streaming) describe surfaces Reverie does not have: its
  API is plain JSON REST with no streaming. And 3.1's JSON Schema 2020-12
  alignment is the better fit for the RFC 7807 / merge-patch shapes the
  [JSON API conventions ADR](2026-05-22-json-api-conventions.md) already fixed.

The shapes the spec describes are those already fixed by the
[JSON API conventions ADR](2026-05-22-json-api-conventions.md) and are not
restated here. The RFC 7807 → RFC 9457 error-envelope refresh is tracked in
the RFC 9457 error-envelope refresh task and is out of this ADR's scope.

The generator choice (e.g. annotation-driven extraction), the spec-to-reference
renderer, and the CI gate wiring are implementation concerns owned by the
docs-as-done epic (the docs-as-done task), not this
decision.

### Consequences

- Good, because path versioning is the most discoverable and proxy-friendly
  scheme, and external clients get an unambiguous contract URL.
- Good, because a generated OpenAPI document gives third-party clients codegen
  and keeps the reference zero-drift, satisfying docs-as-done.
- Good, because adopting both pre-release costs one prefix move and an annotation
  pass; retrofitting either after external clients exist would be a breaking
  rollout.
- Bad, because moving every route from `/api/*` to `/api/v1/*` touches the
  backend mounts, the frontend API client, and tests in one change, which is acceptable
  pre-release, where no external caller is pinned to the old prefix.
- Neutral, because the bundled frontend is lockstep and never exercises the
  version negotiation; `v1` primarily serves external clients and future
  generations.
- Neutral, because 3.2's streaming/querystring features are deferred until
  Reverie has a surface that needs them and a code-first generator emits 3.2;
  3.1 covers the current plain-JSON API fully.

### Confirmation

The JSON data API is served under `/api/v1`; operational and OPDS paths are
unversioned. An OpenAPI 3.1 document is generated from the handler code (no
hand-maintained spec), and both the API reference and the CI docs gate consume
that generated document.

## Pros and Cons of the Options

### V1: URL path version

- Good, because it is discoverable, cacheable, and needs no content negotiation.
- Good, because a new generation (`/api/v2`) can run beside the old one.
- Bad, because the version is coarse (whole-API major), not per-resource: fine
  for a single coherent surface.

### V2: header / `Accept` versioning

- Good, because URLs stay stable across versions.
- Bad, because it is invisible in a browser/curl, harder to cache, and adds
  negotiation logic: cost without benefit for a small, coherent API.

### V3: no versioning

- Good, because it is the least work today.
- Bad, because the first breaking change with an external client in the field has
  no non-breaking escape hatch; the whole point of stamping `v1` now is to keep
  that door open cheaply.

### S1: generated OpenAPI 3.1 (code-first)

- Good, because the code is the single source of truth; the spec and reference
  cannot drift from it.
- Good, because 3.1 is exactly what the code-first Rust generator (utoipa) emits
  today: no upstream wait, no unachievable target.
- Bad, because annotations live in the handlers, coupling spec detail to handler
  code.

### S2: hand-written OpenAPI

- Good, because full control over the document, including authoring 3.2 ahead of
  generator support.
- Bad, because it is a second source of truth that drifts the moment a handler
  changes without a matching spec edit: the failure docs-as-done forbids.

### S3: no spec

- Good, because nothing to maintain.
- Bad, because the reference must be hand-written (drifts) and third-party
  clients get no codegen: fails docs-as-done and the third-party driver.

## More Information

- [JSON API conventions ADR](2026-05-22-json-api-conventions.md): the shapes the
  OpenAPI document describes; this ADR adds the version prefix and the contract
  format without restating those shapes.
- [Single-image distribution ADR](2026-05-05-single-image-distribution-central-csp.md)
  : why the bundled frontend is lockstep and the version mainly serves external
  clients.
- The docs-as-done task: the generator,
  reference renderer, and CI docs gate that implement this decision.
- The RFC 9457 envelope refresh task, which is tracked separately.
- Revisit triggers: adopt OpenAPI 3.2 if Reverie gains a surface that needs it
  (streaming / Server-Sent Events) _and_ a code-first generator emits 3.2; and
  cut `/api/v2` when the first incompatible generation forces it.

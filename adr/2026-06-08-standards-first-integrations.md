---
status: "accepted"
date: 2026-06-08
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Standards-first integrations: expose open interfaces, bundle no adjacent services

## Context and Problem Statement

Reverie runs as one application inside an operator's self-hosted ecosystem, not
as the only thing on the box. That operator already runs an identity provider,
a metrics stack, a log aggregator, e-reader clients, and automation tooling.
When Reverie needs to interoperate with any of those, it faces a recurring
choice: bundle its own version of the adjacent service, or expose a standard
interface the operator's existing tool consumes.

Several integration surfaces already lean the second way: OPDS for catalog
access, OIDC plus forward-auth for identity, `/health` and `/readiness` for
liveness, but the principle behind them was never written down, so each new
integration surface re-opens the bundle-vs-expose question from scratch. This
ADR records the standing philosophy so it does not have to be re-argued per
feature.

## Decision Drivers

- **Reverie is a library manager, not an identity, metrics, or automation
  product.** Bundling those is scope creep and almost always worse than the
  dedicated tool the operator already runs.
- **The operator already owns the consuming side.** Re-implementing an IdP, a
  metrics database, or a log store adds no value and duplicates running infra.
- **Open standards mean zero Reverie-specific glue.** Any conformant tool works
  without bespoke adapters on either side.
- **"Enable, don't own"**: the same philosophy the
  [scale-stance ADR](2026-06-08-scale-stance-stateless-enable-not-own.md)
  applies to HA, here applied to integrations.

## Considered Options

- **A**: Standards-first and hook-based: expose an open-standard interface (or an
  outbound hook) per axis; bundle none of the consuming services.\*\*
- **B**: Batteries-included: bundle adjacent services (built-in user management /
  SSO, a built-in metrics+dashboard stack, a built-in notification engine).\*\*
- **C**: Bespoke per-integration APIs: a custom, non-standard interface for each
  axis.\*\*

## Decision Outcome

Chosen option: **A**. For each axis where Reverie meets the operator's
ecosystem, it exposes an open standard or an outbound hook and lets the
operator's existing tooling consume it; it ships none of the consuming
infrastructure:

- **Catalog / reading**: OPDS feed → any OPDS e-reader. Reverie does not build
  a reader client.
- **Identity**: OIDC + forward-auth headers → the operator's IdP. Reverie does
  not build an SSO / user-directory product.
- **Metrics**: a Prometheus-format `/metrics` endpoint (opt-in) → the
  operator's Prometheus. Reverie does not bundle a metrics store or dashboards.
- **Logs**: structured, machine-parsable logs to stdout → the operator's
  aggregator. Reverie does not bundle log storage.
- **Eventing**: outbound webhooks (the outbound webhooks task,
  the first concrete instance) → the operator's automation. Reverie does not
  build an in-app notification or automation engine.

The rule generalises to any future integration surface: prefer an existing open
standard; if none fits, an outbound hook; bundling a consuming service is the
last resort and requires its own ADR justifying why no standard or hook
suffices.

This ADR governs how Reverie exposes _itself_ to the operator's ecosystem. It
does not cover inbound metadata enrichment (Reverie consuming upstream APIs such
as OpenLibrary or Google Books for cataloguing), which is a separate concern
with its own architecture.

### Consequences

- Good, because Reverie stays focused on the library domain and inherits the
  reliability of the operator's dedicated tools for everything adjacent.
- Good, because any standards-conformant tool integrates with no Reverie-side
  adapter, and operators are not forced onto a Reverie-blessed stack.
- Good, because it is consistent with the project-wide "enable, don't own"
  philosophy, so the integration story is predictable across axes.
- Bad, because Reverie offers no turnkey experience for an operator who runs none
  of the consuming tools: they must stand up (e.g.) a Prometheus or an IdP to
  use those surfaces. Acceptable for the self-hosting audience.
- Neutral, because each new integration must be designed against a standard or a
  hook, which is more up-front thought than emitting a bespoke endpoint.

### Confirmation

Each integration axis is an open standard or an outbound hook, never a bundled
consuming service: OPDS (catalog), OIDC + forward-auth (identity),
Prometheus-format `/metrics` (metrics), structured stdout logs (logging),
outbound webhooks (eventing). Adding a _bundled_ adjacent service requires a new
or superseding ADR; absent one, the standards-first / hook-based shape is the
review baseline.

## Pros and Cons of the Options

### A: standards-first, hook-based, bundle nothing

- Good, because it keeps scope on the library domain and reuses the operator's
  existing, better-maintained tools.
- Good, because open standards remove per-integration glue on both sides.
- Neutral, because it asks the operator to bring the consuming tools.
- Bad, because there is no zero-dependency turnkey experience for those surfaces.

### B: batteries-included bundled stack

- Good, because a fresh operator gets identity, metrics, and notifications with
  no external setup.
- Bad, because it duplicates infrastructure the target audience already runs, and
  a bundled clone is almost always worse than the dedicated tool.
- Bad, because it explodes scope and maintenance into domains (identity, metrics,
  automation) far from a library manager.

### C: bespoke per-integration APIs

- Good, because each interface can be shaped exactly to one need.
- Bad, because every consumer needs a Reverie-specific adapter, the opposite of
  the zero-glue goal, and it forgoes the ecosystem of tools that already speak
  the standards.

## More Information

- [Scale-stance ADR](2026-06-08-scale-stance-stateless-enable-not-own.md): the
  same "enable, don't own" philosophy on the HA axis.
- The outbound webhooks task: outbound webhooks, the first
  concrete eventing integration; implementation tracked there, not here.
- Existing instances of this philosophy (not re-derived here): the OPDS feed,
  OIDC + forward-auth identity, and the `/health` / `/readiness` endpoints
  already shipped.
- Revisit trigger: a specific integration where no open standard or outbound hook
  can meet a real operator need is the signal to write a narrow ADR for bundling
  that one service, not to amend this philosophy by exception.

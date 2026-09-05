---
type: ADR
profile-version: 1
id: "REV-ADR-0022"
title: "Standards-first integrations over bundled adjacent services"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-08"
decision-makers:
  - "John Unkovich"
---

# Standards-first integrations over bundled adjacent services

## Context and problem statement

Reverie runs as one application inside an operator's self-hosted ecosystem, not as the only thing on the box. That
operator already runs an identity provider, a metrics stack, a log aggregator, e-reader clients, and automation
tooling. When Reverie needs to interoperate with any of those, it faces a recurring choice: bundle its own version of
the adjacent service, or expose a standard interface the operator's existing tool consumes.

Several integration surfaces already lean the second way: OPDS for catalog access, OIDC plus forward-auth for
identity, `/health` and `/health/ready` for liveness, but the principle behind them was never written down, so each
new integration surface re-opens the bundle-vs-expose question from scratch. This record fixes the standing
philosophy so it does not have to be re-argued per feature.

## Decision drivers

- Reverie is a library manager, not an identity, metrics, or automation product. Bundling those is scope creep and
  almost always worse than the dedicated tool the operator already runs.
- The operator already owns the consuming side. Re-implementing an IdP, a metrics database, or a log store adds no
  value and duplicates running infrastructure.
- Open standards mean zero Reverie-specific glue. Any conformant tool works without bespoke adapters on either side.
- "Enable, don't own": the same philosophy the [scale-stance record](./0021-scale-stance-stateless-application-operator-enabled-ha.md)
  applies to high availability, here applied to integrations.

## Considered options

- Standards-first, hook-based integration: expose an open-standard interface, or an outbound hook where no standard
  fits, per axis; bundle none of the consuming services.
- Batteries-included bundled services: bundle adjacent services (built-in user management and SSO, a built-in
  metrics and dashboard stack, a built-in notification engine).
- Bespoke per-integration APIs: a custom, non-standard interface for each axis.

## Decision outcome

Chosen option: **Standards-first, hook-based integration**, because it keeps Reverie's scope on the library domain
and lets each axis reuse a tool the operator already runs, at the cost of a turnkey experience for an operator who
runs none of those tools.

For each axis where Reverie meets the operator's ecosystem, the chosen design exposes an open standard where one
fits, or an outbound hook where none does, and leaves the consuming infrastructure to the operator's existing
tooling:

- Catalog and reading: an OPDS feed, consumed by any OPDS-capable e-reader. Reverie does not build a reader client.
- Identity: OIDC plus forward-auth headers, consumed by the operator's identity provider. Reverie does not build an
  SSO or user-directory product.
- Metrics: an open, scrape-friendly metrics interface, consumed by the operator's existing metrics stack. Reverie
  does not bundle a metrics store or dashboards.
- Logs: structured logs to stdout, consumed by the operator's log aggregator. Reverie does not bundle log storage.
- Eventing: an outbound hook, consumed by the operator's automation. Reverie does not build an in-app notification
  or automation engine.

OPDS and OIDC plus forward-auth are the two axes already built this way today.

The rule generalises to any future integration surface: prefer an existing open standard; if none fits, an outbound
hook; bundling a consuming service is the last resort and needs its own decision record justifying why no standard
or hook suffices.

This record governs how Reverie exposes itself to the operator's ecosystem. It does not cover inbound metadata
enrichment (Reverie consuming upstream APIs such as OpenLibrary or Google Books for cataloguing), which is a
separate concern with its own architecture.

### Consequences

- Positive: Reverie stays focused on the library domain and inherits the reliability of the operator's dedicated
  tools for everything adjacent.
- Positive: any standards-conformant tool integrates with no Reverie-side adapter, and operators are not forced onto
  a Reverie-blessed stack.
- Positive: consistent with the project-wide "enable, don't own" philosophy, so the integration story is predictable
  across axes.
- Negative: Reverie offers no turnkey experience for an operator who runs none of the consuming tools: they must
  stand up, for example, a Prometheus instance or an identity provider to use those surfaces. Acceptable for the
  self-hosting audience.
- Negative: each new integration surface must be designed against a standard or a hook, which is more up-front
  thought than emitting a bespoke endpoint.

## Pros and cons of the options

### Standards-first, hook-based integration

- Positive: keeps scope on the library domain and reuses the operator's existing, better-maintained tools.
- Positive: open standards remove per-integration glue on both sides.
- Neutral: asks the operator to bring the consuming tools.
- Negative: there is no zero-dependency turnkey experience for those surfaces.

### Batteries-included bundled services

- Positive: a fresh operator gets identity, metrics, and notifications with no external setup.
- Negative: duplicates infrastructure the target audience already runs, and a bundled clone is almost always worse
  than the dedicated tool.
- Negative: explodes scope and maintenance into domains (identity, metrics, automation) far from a library manager.

### Bespoke per-integration APIs

- Positive: each interface can be shaped exactly to one need.
- Negative: every consumer needs a Reverie-specific adapter, the opposite of the zero-glue goal, and it forgoes the
  ecosystem of tools that already speak the standards.

## More information

- [Scale-stance record](./0021-scale-stance-stateless-application-operator-enabled-ha.md): the same "enable, don't
  own" philosophy on the high-availability axis.
- A specific integration where no open standard or outbound hook can meet a real operator need is the signal to
  write a narrow decision record for bundling that one service, not to amend this philosophy by exception.

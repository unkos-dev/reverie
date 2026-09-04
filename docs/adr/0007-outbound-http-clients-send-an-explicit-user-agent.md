---
type: ADR
profile-version: 1
id: "REV-ADR-0007"
title: "Outbound HTTP clients send an explicit User-Agent"
status: "accepted"
recorded-on: "2026-09-04"
decided-on: "2026-05-18"
decision-makers:
  - "John Unkovich"
informed:
  - "Reverie contributors"
---

# Outbound HTTP clients send an explicit User-Agent

## Context and problem statement

Reverie was deployed against an Authentik instance fronted by Cloudflare, and OIDC discovery failed at startup with a
`403 Forbidden`. Zone-API inspection and a sibling-container reproduction traced the cause: `reqwest` sends no default
`User-Agent` header when `.user_agent(...)` is never called on `ClientBuilder`, and Cloudflare's default scanner
blocklist includes the predicate `http.user_agent eq ""`, so a request with no `User-Agent` is dropped with a 403
before it reaches the origin. Two of Reverie's outbound HTTP clients, the OIDC discovery and token-exchange clients in
`backend/src/auth/oidc.rs`, were built via that path, with no User-Agent set. The enrichment clients in
`backend/src/services/enrichment/http.rs` already set a User-Agent derived from `config.user_agent()`. An audit at fix
time found no other production outbound HTTP client; the remaining bare `reqwest::Client::new()` call sites are all
under `#[cfg(test)]` and target wiremock on `127.0.0.1`.

The same failure mode applies to any future outbound client added to Reverie, against any WAF that scores an
empty-User-Agent request as suspicious; Cloudflare, AWS WAF, Akamai, and Fastly all ship variants of this rule.
Without a project-level convention, the next contributor adding an outbound client has no way to know this is a
load-bearing call, and the failure resurfaces as a deployment-substrate problem rather than something code review can
catch.

## Decision drivers

- Reachability behind common WAFs: Reverie is open-source and self-hosted, and the threat model assumes operators
  place it behind whichever WAF or CDN their network already runs, so the default client must work against that
  surface.
- Upstream-provider traceability: public identity providers and metadata APIs (Google Books, Hardcover, Open
  Library, OPDS feeds) log client User-Agents, and a stable, identifiable one lets upstream operators match
  misbehaviour to a specific deployment without operators having to volunteer it themselves.
- No abuse-cloak: spoofing a browser User-Agent is rejected, because it reduces an upstream operator's ability to
  identify the client, which is bad-citizen behaviour for an open-source HTTP consumer.
- Single load-bearing fact: the rule is small enough that writing it down once costs less than relearning it from a
  future incident.

## Considered options

- Project convention, enforced by code review: every outbound client sets a `reverie/<version>` User-Agent minimum,
  with provider-courtesy contact appended where relevant.
- Wrapper crate or shared builder in `backend/src/http/` that funnels every outbound client through one constructor.
- Compile-time lint that forbids a `ClientBuilder` with no `.user_agent(...)` call.
- Do nothing, and document the requirement in the WAF-deployment guide instead.

## Decision outcome

Chosen option: **project convention, enforced by code review**, because the alternatives cost more than a
six-call-site codebase justifies: a shared builder adds a project-internal abstraction the two client shapes do not
need, and a compile-time lint has no off-the-shelf implementation to build on.

Every outbound HTTP client in Reverie's production code sets an explicit User-Agent header. Clients with no
operator-configurable identity available at construction time, namely OIDC discovery and OIDC token exchange, set the
floor `reverie/<CARGO_PKG_VERSION>` via `concat!("reverie/", env!("CARGO_PKG_VERSION"))` on the `ClientBuilder`.
Clients that hit third-party metadata APIs (Google Books, Hardcover, Open Library) append the operator-configured
contact string from `config.user_agent()`, in the form `reverie/<version> (+<contact_url_or_email>)`. A `reqwest`
client with no `.user_agent(...)` call, which sends no `User-Agent` header at all, does not appear in production
code; the only remaining bare `reqwest::Client::new()` call sites target wiremock on loopback under `#[cfg(test)]`,
where the exemption holds because wiremock does not score User-Agents and the test surface is not exposed to WAF
rules.

This record captures the convention. Per-site enforcement was by code review at the decision; the escalation
condition it set (a second lapse) has since fired, and the `disallowed-methods` entries in `backend/clippy.toml`
now back the convention by banning the bare `reqwest` constructors. The OIDC module carries the constraint in its
top-of-file `//!` docs so a future reader meets it without first finding this record.

### Consequences

- Positive: OIDC discovery succeeds against any WAF that drops empty-User-Agent requests, and the same class of
  failure is pre-empted for every outbound client added afterwards.
- Positive: upstream identity-provider and API operators see a stable, traceable client.
- Positive: the convention needs no new dependency, no new abstraction, and no new lint to maintain.
- Positive: bumping the crate version rolls the User-Agent automatically, so there is no separate version pin to
  maintain.
- Negative: code-review enforcement is fallible; a future pull request that adds an outbound client without a
  User-Agent can land if the reviewer does not know to look for it. Two such lapses would be grounds to revisit the
  compile-time-lint or shared-builder options.
- Negative: the OIDC User-Agent carries no operator contact, because discovery happens before operator-contact
  configuration is available and threading it through costs more than the marginal benefit; operator contact appears
  only on the enrichment clients.

### Confirmation

`backend/clippy.toml` bans `reqwest::Client::new`, `reqwest::Client::builder`, and `reqwest::ClientBuilder::new` as
disallowed methods, so every production client is built through one of the sanctioned constructors that set the
header. `backend/src/auth/oidc.rs` carries the
regression test `http_client_sends_reverie_user_agent`, which fails if the OIDC client's User-Agent is removed or
changes shape.

## Pros and cons of the options

### Project convention, enforced by code review

- Positive: no new dependency, abstraction, or lint for a six-call-site concern; the regression test and the
  disallowed-method entries catch the common lapse.
- Negative: enforcement at a new call site depends on the reviewer knowing the convention.

### Wrapper crate or shared builder

- Positive: stronger enforcement than code review, since every outbound client is funnelled through one constructor.
- Negative: introduces a project-internal abstraction for a one-line concern; the enrichment client already takes
  the User-Agent as an argument, and the OIDC client has no `Config` in scope at construction time on some call
  paths.

### Compile-time lint

- Negative: no off-the-shelf lint existed for this rule at the decision, and writing one for a six-call-site
  codebase was disproportionate to the problem; the `disallowed-methods` entries in `backend/clippy.toml` later took
  that role once the escalation condition fired.

### Do nothing, document in the deployment guide

- Negative: pushes the burden onto every operator, and this was not the first substrate-edge-case failure of this
  kind in Reverie's deploy path.

## More information

Implementation sites: `backend/src/auth/oidc.rs` (the fixed clients), `backend/src/services/enrichment/http.rs` (the
pre-existing conformant clients and the pattern source for the provider-courtesy User-Agent shape), and
`backend/src/config/mod.rs::user_agent` (the operator-contact User-Agent composition used by the enrichment clients).

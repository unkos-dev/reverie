---
status: accepted
date: 2026-05-18
supersedes: []
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# Outbound HTTP clients in Reverie must send an explicit `User-Agent`

## Context and Problem Statement

The outbound HTTP user agent issue surfaced a
startup-time `403 Forbidden` on OIDC discovery when Reverie was
deployed against an Authentik instance fronted by Cloudflare. The
root cause, confirmed via empirical CF zone-API inspection plus
sibling-container repro, is:

- `reqwest` does **not** populate a default `User-Agent` header when
  `.user_agent(...)` is not invoked on `ClientBuilder`. A
  `ClientBuilder::new().build()` chain sends no `User-Agent` header
  at all.
- Cloudflare's default scanner blocklist includes the predicate
  `http.user_agent eq ""`, so any outbound request that does not set
  a `User-Agent` is dropped with a 403 before reaching the origin.
- Two of Reverie's outbound HTTP clients
  (`backend/src/auth/oidc.rs::http_client` and
  `exchange_http_client`) were built via the unconfigured-UA path.
  The four enrichment clients
  (`backend/src/services/enrichment/http.rs::api_client`,
  `::cover_client`, and their callers) already set a UA derived
  from `config.user_agent()`. The audit at fix time confirmed no
  other production outbound HTTP clients exist; the bare
  `reqwest::Client::new()` sites all live in `#[cfg(test)]` and
  target wiremock on `127.0.0.1`.

The same failure mode applies to any future outbound client added
to Reverie, against any WAF that scores empty-UA requests as
"suspicious" (Cloudflare, AWS WAF, Akamai, Fastly all ship variants
of this rule). Without a project-level convention, the next
contributor adding an outbound client has no way to know this is a
load-bearing call, and the regression resurfaces silently as a
deployment-substrate failure rather than a code-review-catchable
defect.

## Decision Drivers

- **Reachability behind common WAFs.** Reverie is open-source and
  self-hosted; the threat model assumes operators put Reverie
  behind whichever WAF/CDN their network already runs. The default
  client must work against that surface area.
- **Upstream-provider traceability.** Public IdPs and metadata
  APIs (Google Books, Hardcover, Open Library, OPDS feeds) log
  client UAs. A stable, identifiable UA lets upstream operators
  match misbehaviour to a specific deployment without forcing
  Reverie operators to volunteer it.
- **No abuse-cloak.** The opposite anti-pattern, spoofing a
  browser UA, is rejected: it reduces upstream operators' ability
  to identify the client, which is bad-citizen behaviour for an
  OSS HTTP consumer.
- **Single load-bearing fact.** The "set an explicit UA" rule is
  small enough that the cost of writing it down once exceeds the
  cost of relearning it from incidents.

## Considered Options

- **Option A: Project convention: every outbound client sets a
  `reverie/<version>` UA minimum, with provider-courtesy contact
  appended where relevant.** Captured in this ADR and enforced via
  per-site code review.
- **Option B: Wrapper crate / shared builder in
  `backend/src/http/`.** Funnels all outbound clients through a
  single constructor that injects the UA. Stronger enforcement but
  introduces a project-internal abstraction for a one-line concern;
  the enrichment client already takes the UA as an arg, and the
  OIDC client doesn't have a `Config` in scope at construction
  time (it's called from `config.rs` itself in some paths).
- **Option C: Compile-time lint.** A `dylint` / `clippy.toml`
  rule that forbids `ClientBuilder::new().build()` without
  `.user_agent(...)`. Possible but heavy; no off-the-shelf lint
  exists and writing one for a 6-call-site codebase is
  over-engineering.
- **Option D**: Do nothing, document in the WAF-deploy guide.
  Pushes the burden onto every operator. Already proven failure
  mode (the user agent issue was the second time this kind of substrate-edge
  case has bitten us, after the GLIBC dynamic linker issue).

## Decision

**Option A.** Every outbound HTTP client in Reverie production
code sets an explicit `User-Agent` header:

- **Minimum:** `reverie/<CARGO_PKG_VERSION>`, set via
  `concat!("reverie/", env!("CARGO_PKG_VERSION"))` on the
  `ClientBuilder`. This is the floor for clients that have no
  operator-configurable identity attached (OIDC discovery, OIDC
  token exchange).
- **Provider-courtesy (enrichment):** clients hitting third-party
  metadata APIs (Google Books, Hardcover, Open Library) append
  the operator-configured contact string per
  `config.user_agent()`. The format is
  `reverie/<version> (+<contact_url_or_email>)`.

The `reqwest`-default-UA behaviour (no header at all) is forbidden
in production code. Test-only clients targeting wiremock on
loopback (`reqwest::Client::new()` under `#[cfg(test)]`) are
exempt; wiremock does not score UAs and the test surface area is
not affected by WAF rules.

This ADR records the convention; per-site enforcement is by code
review. The OIDC module carries the threat annotation in its
top-of-file `//!` docs to flag the constraint to future readers
without requiring them to find this ADR first.

## Consequences

### Good

- OIDC discovery succeeds against any WAF that drops empty-UA
  requests, which was fixed and the same class of failure
  pre-empted for every future outbound client.
- Upstream IdP / API operators see a stable, traceable client.
- The convention is small enough to encode in code review with no
  new dependencies, no new abstractions, and no new lints to
  maintain.

### Bad

- Code-review enforcement is fallible. A future PR that adds an
  outbound client without a UA can land if the reviewer doesn't
  know to look. If this happens twice, escalate to Option C
  (compile-time lint) or Option B (mandatory wrapper).
- The OIDC UA does not include operator contact: discovery is
  before operator-contact config is plumbed, and threading it
  through is more code-churn than the marginal benefit. Operator
  contact appears only on enrichment clients.

### Neutral

- Bumping the crate version automatically rolls the UA. No
  separate version pin to maintain.

## Validation

- `backend/src/auth/oidc.rs::tests::http_client_sends_reverie_user_agent`
  is a regression test that fails if the OIDC client's UA is
  removed or changes shape.
- Homelab `auth.unkos.net` substrate run against
  `sha-<fix-commit>` flips reverie container health from
  `unhealthy` (crash-loop on OIDC discovery 403) to `healthy`.

## References

- Outbound HTTP user agent investigation and fix.
- Homelab substrate deploy that surfaced the failure mode.
- Predecessor:
  the GLIBC dynamic linker fix:
  same class of substrate-edge-case bug in the same deploy
  pipeline, motivating the longer-form decision record here
  rather than yet another incident-only fix.
- `backend/src/auth/oidc.rs`: implementation site.
- `backend/src/services/enrichment/http.rs`: pre-existing
  conformant client; pattern source for the provider-courtesy
  UA shape.
- `backend/src/config.rs::user_agent`: operator-contact UA
  composition for enrichment clients.

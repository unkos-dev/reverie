---
type: ADR
profile-version: 1
id: "REV-ADR-0028"
title: "API authorization: orthogonal scope, role, and ownership axes"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-23"
decision-makers:
  - "John Unkovich"
---

# API authorization: orthogonal scope, role, and ownership axes

## Context and problem statement

Reverie's API has no first-class authorization representation. Role requirements are expressed only as documented
`403` responses in handler prose, not as a contract artifact. The generated OpenAPI document
([API versioning and OpenAPI](./0016-api-versioning-by-url-path-with-openapi-as-the-contract.md)) describes shapes
and status codes but carries no model of what capability a credential must hold to reach an endpoint. A third-party
client reading the spec cannot tell that `POST /api/v1/tokens` needs administrative capability except by provoking
the `403` at runtime.

The credential layer is equally flat. Device tokens (random 256-bit value, SHA-256 hashed, presented over HTTP
Basic) carry no scopes: any valid token can reach any endpoint the underlying user's role permits, with no way to
mint a read-only or otherwise narrowed credential for automation. The request extractor (`CurrentUser`) resolves a
caller from the session cookie or HTTP Basic into a single identity carrying `user_id`, `role`, and `is_child`, then
gates with `require_admin` and `require_not_child`. Identity gating exists; delegated-capability authorization does
not.

Adding scoped tokens and validating IdP-issued JWT access tokens for API callers touches the credential schema, the
extractor, the API contract, and the security posture at once. The authorization model needed fixing before those
surfaces were built, so that scopes, roles, and ownership stay distinct rather than collapsing into one ad-hoc check
per handler.

How should API authorization be modelled, represented in the contract, and enforced, so that capability, identity,
and resource ownership remain separable and every check lands server-side?

This decision covers API authorization only. Authentication, identity, sessions, bootstrap, and recovery are a
separate decision (see More information).

## Decision drivers

- Capability authorization (what a credential may do) and identity gating (who the caller is) are different
  concerns and should not be conflated into a single per-handler check.
- The administrative surface needs a contract representation a client can read, replacing the interim "documented
  `403`" practice that is invisible until a request fails.
- Scoped credentials for automation and reader clients: a token should be mintable as read-only without granting
  the user's full role capability.
- Standards-default: OAuth 2.0 scopes for delegated capability (RFC 6749 section 3.3), role-based gating kept
  separate, the RFC 9068 profile for validating IdP-issued JWT access tokens, per the project's
  industry-standard-default principle.
- Authorization is a server-side property. Client-side checks are presentation only and never load-bearing.
- Compatibility: existing reader and OPDS clients authenticate over HTTP Basic and must keep working unchanged.
- Supply-chain posture: the resource-server validation path must rest on maintained crates compatible with the
  project's axum 0.8, not abandoned or version-pinning ones.

## Considered options

- Three orthogonal axes (scope, role and child status, ownership), all enforced server-side, with `jsonwebtoken`
  and `jwks_client_rs` for resource-server JWT validation
- Role-only gating (the status quo), with the administrative surface left as documented `403` responses
- Scopes only, folding role and child distinctions into additional scopes
- `jwt-authorizer`, an axum-integrated all-in-one JWT validator
- Hand-rolled JWKS fetch and signature verification

## Decision outcome

Chosen option: **three orthogonal axes (scope, role and child status, ownership), all enforced server-side, with
`jsonwebtoken` and `jwks_client_rs` for resource-server JWT validation**, because it keeps capability, identity,
and ownership separable while resting resource-server validation on maintained crates compatible with axum 0.8.

API authorization is expressed on three orthogonal axes, kept distinct and all enforced server-side:

1. **Scope is credential capability.** A typed enum: `read` (safe methods), `write` (mutations), `admin`
   (administrative endpoints). This is delegated-capability authorization in the OAuth sense (RFC 6749 section
   3.3): it bounds what a given credential may do, independent of who holds it.
2. **Role and child status are identity gating.** The administrative surface, the existing not-child restriction,
   and content-visibility filtering are enforced from the user's role and child flag, server-side, regardless of
   the credential's scope.
3. **Ownership is the resource axis.** A caller acts only on rows they own, enforced at the data layer (row-level
   security and ownership predicates), not in the handler-level capability check.

These axes are independent: a request must satisfy all three to succeed. Scope answers "may this credential
perform writes at all"; role and child status answer "is this caller permitted to reach the administrative or
adult-only surface"; ownership answers "is this specific row theirs to touch". Collapsing them into scopes alone
would force child restrictions and administrative gating to be re-encoded as scopes, losing the clean separation
between a credential's delegated capability and the identity behind it.

**Scope representation.** Scopes are a typed enum (`read`, `write`, `admin`) persisted as an array of the Postgres
`scope` enum on the credential. Within the scope axis the three values compose as a hierarchy, `read < write <
admin`: a higher scope subsumes every lower one, so an endpoint gates on the least scope it requires and any
credential holding that scope or higher clears it. `read` is the floor, so every valid credential carries at least
`read` and a scopeless credential is rejected at authentication. Sessions derive their scope set from the user's
role (an interactive login gets the full capability its role allows). Tokens carry explicit scopes, bounded by the
user's role ceiling.

**Role-to-scope ceiling.** Administrator unlocks the `admin` scope; `read` and `write` are available to all roles.
A child account holds `read` and `write` scope so it can manage its own settings and shelves. Child restrictions
come from the role and not-child gates and from ownership, never from withholding scope. Scope bounds capability;
it is not the mechanism for age gating. A token may only request scopes its owner's role permits, so a
non-administrator cannot mint an `admin`-scoped token.

**Scoped tokens.** The existing device tokens evolve rather than a new credential type being introduced: a
`scopes` column on `device_tokens` defaults to `read`. Existing tokens become read-only by default, which is the
safe direction for a capability that did not exist before. This narrows any token that previously issued writes,
acceptable pre-release where migrations consolidate and no external client is pinned to the prior unscoped
behaviour. The token's hashing (SHA-256 of a 256-bit random value, constant-time compared) is unchanged: that is
correct for a high-entropy random token and orthogonal to scope.

**Token transport.** HTTP Basic remains for reader and OPDS clients (compatibility). New scoped personal tokens
and resource-server JWTs use Bearer. All transports resolve through the same extractor to the same in-process
identity and the same scope set, so authorization logic downstream is transport agnostic.

**Unified credential format and indexed resolution.** The plaintext credential is `{prefix}{token_id}.{secret}`
for Bearer and `{prefix}{token_id}:{secret}` for Basic, split into a username/password pair on the same delimiter.
`token_id` is the device token's own row id, so both transports resolve through one indexed lookup by id. This
replaces the prior per-user scan over hashed secrets and the constant-time comparison that scan needed to stay
safe. A token also carries an optional expiry (`expires_at`); the same lookup excludes an expired, non-revoked
token exactly as it excludes a revoked one, so expiry sits alongside revocation as a first-class exclusion, not a
check bolted on afterward.

**Resource-server validation (the RFC 9068 profile).** Inbound JWT access tokens issued by a configured IdP are
validated with `jsonwebtoken` for the underlying signature, expiry, and not-before checks, and `jwks_client_rs`
(built on `jsonwebtoken`) for a cached, rotating key set fetched from the configured issuer. Those libraries do not
enforce the issuer claim and check the audience only when one is passed, so a first-party wrapper enforces the
issuer and always passes the audience, and pins the algorithm from the trusted key (the JWK) rather than the token
header, so a token cannot downgrade or switch the verification algorithm. The key-set URL is never taken from
token content: no `jku` or `x5u` header is followed. The JWKS URL comes from configuration or OIDC discovery only.
`jwt-authorizer` is rejected: it is unmaintained since 2024 and pins axum 0.7 against the project's axum 0.8
(`backend/Cargo.toml`, `axum = "0.8.9"`), which would either block the build or force a downgrade of the framework.
Hand-rolling JWKS fetch and verification is rejected as security-critical code that the two maintained crates
already cover.

**OpenAPI representation.** Scopes are modelled first-class in the generated OpenAPI contract: security schemes
and per-operation scope requirements, so the administrative surface is described in the spec rather than
discovered through a runtime `403`. This replaces the interim representation in which the administrative surface
existed only as documented `403` responses. The shapes and version prefix fixed by
[JSON API conventions](./0011-json-api-conventions-for-the-browser-facing-rest-surface.md) and
[API versioning and OpenAPI](./0016-api-versioning-by-url-path-with-openapi-as-the-contract.md) are unchanged; this
decision adds the authorization model that contract now carries.

### Consequences

- Positive: capability, identity, and ownership stay separable: each axis has one enforcement point, so a handler
  reasons about one concern at a time and the model survives future role or scope changes.
- Positive: narrowed credentials become expressible: a read-only token for automation is mintable without granting
  the holder's full role capability, and existing tokens default to the least-capable scope.
- Positive: the administrative surface is now contract-visible: a client generating against the OpenAPI document
  sees scope requirements instead of learning them from runtime failures.
- Positive: IdP-issued access tokens are validated against a standard profile (RFC 9068) on maintained crates
  compatible with axum 0.8, with no abandoned dependency on the auth-critical path.
- Negative: three axes is more enforcement surface than role-only gating: scope checks, identity gates, and
  ownership predicates must each be present. The orthogonality is the mitigation: each axis is checked in one
  place.
- Negative: the resource-server path adds a wrapper that must enforce issuer and audience itself; relying on the
  libraries' defaults would silently accept tokens from the wrong issuer or for the wrong audience.
- Negative: `jwks_client_rs` validates signature, expiry, and not-before, but does not validate the issuer, and
  validates the audience only when one is passed, so that responsibility sits with the first-party wrapper rather
  than the library.

## Pros and cons of the options

### Three orthogonal axes (scope, role and child status, ownership), all enforced server-side, with `jsonwebtoken` and `jwks_client_rs` for resource-server JWT validation

- Positive: capability, identity, and ownership each have a single enforcement point and do not entangle.
- Positive: it maps onto established practice: OAuth scopes for delegated capability, RBAC for identity gating,
  row-level ownership at the data layer.
- Positive: scoped credentials and the administrative surface both gain a contract representation without
  disturbing role and ownership logic.
- Positive: both JWT crates are maintained and compatible with axum 0.8, and the split is honest: the libraries do
  signature, algorithm, expiry, and not-before; the wrapper owns issuer and audience, which the libraries do not
  enforce by default.
- Neutral: it is more model than role-only gating; the cost is paid once in the extractor and the data layer, not
  per handler.
- Neutral: the wrapper is first-party security code, but small and narrowly scoped to the load-bearing invariant.

### Role-only gating (the status quo), with the administrative surface left as documented `403` responses

- Positive: it is the status quo and needs no new credential surface.
- Negative: credentials cannot be narrowed: every token carries the full capability of its user's role, so a
  read-only automation token is impossible.
- Negative: the administrative surface stays invisible in the contract, discoverable only by triggering a `403`.

### Scopes only, folding role and child distinctions into additional scopes

- Positive: there is a single axis to reason about.
- Negative: role and child distinctions must be re-expressed as scopes, conflating "what this credential may do"
  with "who this caller is" and blurring age gating into capability, which is where the model leaks.
- Negative: ownership still cannot be a scope: it is per-row, so a third axis reappears regardless.

### `jwt-authorizer`, an axum-integrated all-in-one JWT validator

- Positive: it is a single axum-integrated dependency that bundles JWKS fetch, caching, and validation.
- Negative: it is unmaintained since 2024 and pins axum 0.7 against the project's axum 0.8: adopting it blocks the
  build or forces a framework downgrade, the same abandoned-wrapper-on-the-auth-path trap the
  [first-party session layer](./0015-first-party-session-layer-on-the-tower-sessions-core.md) decision refused.

### Hand-rolled JWKS fetch and signature verification

- Positive: it adds no dependency.
- Negative: it re-implements key-set caching, rotation, and signature verification: security-critical code the
  maintained crates already cover, for no benefit.

## More information

[JSON API conventions](./0011-json-api-conventions-for-the-browser-facing-rest-surface.md) scoped its CSRF
synchronizer-token defence to browser cookie-authed operations. Header-authenticated callers (HTTP Basic, Bearer)
sit outside that defence by the nature of the synchronizer pattern, since a browser does not auto-attach an
Authorization header the way it auto-sends a cookie; this decision adds the scope model those header-authenticated
credentials carry.

Extends, and does not replace,
[first-party session layer](./0015-first-party-session-layer-on-the-tower-sessions-core.md): sessions established
there derive their scope from the user's role under this model.

The companion [authentication and identity](../../adr/2026-06-23-auth-identity-pluggable-providers.md) decision
covers providers, sessions, bootstrap, recovery, and account linking, including the resource-server issuer,
audience, and scope-mapping configuration this decision's validation path consumes. The two decisions share the
in-process identity that all transports resolve to.

Out of scope, deferred to later decisions: multi-factor authentication, and trusting edge-asserted identity
(consuming identity headers asserted by an upstream proxy). The backend trusts only its own credentials today, and
any move to trust an edge assertion is a separate decision.

Standards basis: RFC 6749 section 3.3 (OAuth 2.0 access-token scope), RFC 9068 (JWT profile for OAuth 2.0 access
tokens), and the project's industry-standard-default principle (OAuth scopes for delegated capability, RBAC kept
separate, enforcement server-side).

Revisit trigger: if `jwks_client_rs` or `jsonwebtoken` falls out of maintenance or lags an axum bump, re-evaluate
the resource-server crate split against the supply-chain posture, as the session-layer decision did for its
wrappers.

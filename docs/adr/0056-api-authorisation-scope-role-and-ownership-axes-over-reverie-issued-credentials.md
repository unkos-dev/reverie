---
type: ADR
profile-version: 1
id: "REV-ADR-0056"
title: "API authorisation: scope, role and ownership axes over Reverie-issued credentials"
status: "accepted"
supersedes:
  - "REV-ADR-0028"
recorded-on: "2026-10-13"
decision-makers:
  - "John Unkovich"
---

# API authorisation: scope, role and ownership axes over Reverie-issued credentials

## Context and problem statement

Reverie supports local sign-in and optional OpenID Connect sign-in to a canonical account. Browser sign-in establishes a
Reverie session. Reader clients and automation can use scoped tokens issued by Reverie over HTTP Basic or Bearer. These
uses do not require Reverie's API to accept access tokens issued by an external identity provider.

External bearer-token validation adds a separate trust boundary: provider discovery, signing-key retrieval and refresh,
signature and claim validation, issuer and audience configuration, and account resolution. There is no current product
requirement for an independent client or delegated integration that needs this boundary. Maintaining it nevertheless
requires security and operational decisions separate from those needed for browser sign-in.

The scope of this choice is the API credential boundary. It preserves the separation of credential capability, account
role and child restrictions, and resource ownership. It does not change how local or federated sign-in establishes an
account or session.

## Decision drivers

- Support local sign-in and optional OIDC single sign-on for self-hosted instances.
- Keep scoped Reverie-issued tokens and HTTP Basic compatibility for readers and automation.
- Keep scope, role and child status, and ownership checks separate and enforced on the server.
- Avoid maintaining an external-token trust boundary without a concrete integration that needs it.
- Preserve the ability to reconsider external API authorisation against an explicit integration contract.

## Considered options

- Retain scoped Reverie-issued API tokens and retire external bearer-token validation.
- Retain both API token sources and complete the external validator's lifecycle hardening.
- Keep external bearer-token validation as a disabled optional feature.

## Decision outcome

Chosen option: **retain scoped Reverie-issued API tokens and retire external bearer-token validation**, because it
supports the required sign-in, reader and automation workflows without maintaining a second API token authority.

Local sign-in and OIDC sign-in remain available. OIDC authenticates a browser before Reverie establishes its session;
its ID-token verification and signing-key needs remain part of the login boundary. Reverie-issued scoped tokens remain
usable over Bearer and HTTP Basic. External identity-provider access tokens are outside the API's accepted credential
set, including tokens for identities already linked through OIDC sign-in.

The authorisation model retains three independent axes: scope expresses credential capability, role and child status
express account restrictions, and ownership controls the target resource. The read, write and admin hierarchy remains;
token minting remains bounded by the account's role, and a credential's granted scope is not silently widened. Browser
sessions derive their scope from the current account role. Role restrictions and ownership checks remain in force even
when a credential carries the operation's required scope. OpenAPI continues to declare scope requirements for supported
credential transports.

### Consequences

- Positive: local sign-in, OIDC single sign-on, scoped automation and existing reader authentication remain supported.
- Positive: API requests no longer need external signing-key retrieval, JWT validation or provider-to-scope mapping.
- Positive: the API credential set and its configuration become smaller without removing the authorisation axes.
- Negative: clients using identity-provider access tokens must obtain Reverie-issued tokens instead.
- Negative: standardised external delegated API access and centrally managed service credentials are deferred. A future
  integration needs a new decision, implementation and security testing rather than restoration of an old code path.
- Negative: OIDC login still needs its own signing-key lifecycle and failure-policy assessment; removing API token
  validation does not resolve login failures caused by key rotation.

## Pros and cons of the options

### Retain both API token sources

- Positive: independent clients can use the configured identity provider's access tokens for already-linked accounts.
- Negative: Reverie maintains discovery, refresh, cancellation, claim and stale-key policies without a current consumer
  that establishes their required behaviour.

### Keep a disabled optional feature

- Positive: operators can opt into external tokens without a separate implementation.
- Negative: optional activation still exposes a supported security boundary and leaves its dependencies, configuration
  and test obligations in the product. Disabling it by default does not remove that maintenance cost.

## More information

[Unified identity with pluggable authentication providers](./0029-unified-identity-with-pluggable-authentication-providers.md)
continues to govern local sign-in, OIDC sign-in and canonical accounts. Reconsider external API tokens when a concrete
independent client, delegated integration or service-credential workflow requires them.

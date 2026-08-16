---
severity: low
surfaces: [developer]
adopted: 2026-08-14
adopted-because: the only adapter presenting a reqwest 0.13 client to oauth2 is published as a pre-release
lift-when-class: dep-unblocks
lift-when: oauth2-reqwest publishes a stable release compatible with the oauth2 and reqwest majors Reverie resolves
---

# The OIDC transport adapter is pinned to a pre-release

## Constraint

`backend/src/auth/oidc.rs` runs every configured OIDC egress path over one
`reqwest 0.13` client. `openidconnect` cannot take that client directly: its
bundled transport is `oauth2`'s optional `reqwest` feature, which pins
`reqwest 0.12`. The adapter that bridges the gap, `oauth2-reqwest`, is
published only as `0.1.0-alpha.3`, so `backend/Cargo.toml` carries an exact
pin:

```toml
oauth2-reqwest = "=0.1.0-alpha.3"
```

## Why this isn't the right shape

A pre-release version carries no compatibility promise. The adapter performs
request and response conversion on an authentication path that moves an
authorization code and a `client_secret`, so an unreviewed change to that
conversion is security-relevant, and a caret requirement would let one arrive
through a routine lockfile refresh.

The exact pin is the containment, not the problem. It costs a deliberate
review whenever the pin moves, which is the intended behaviour for this
dependency; the debt is that a stable release should make the review ordinary
rather than special.

## Currently carried controls

- The exact pin means no alpha bump reaches the tree without an explicit,
  reviewed change.
- The adapter's API stays private to `auth::oidc`: `OidcTransport` is the only
  type that names it, so replacing it later touches one module.
- `just rust::dependency-graph` fails if the adapter disappears from the graph
  or a second `reqwest` major returns, in `rust::check` and in the backend CI
  job.
- `cargo deny` covers it under the same advisory, license, and source policy as
  every other dependency.

## Rejected alternatives

Copying the adapter into Reverie as a first-party `AsyncHttpClient` impl was
considered and rejected: it is about seventy lines of security-sensitive
request and response conversion that upstream maintains and reviews within the
OAuth ecosystem, and owning a private copy trades a version pin for a
maintenance obligation on exactly the code least suited to it.

Keeping both `reqwest` majors was also rejected. Two HTTP stacks with
independent TLS configuration behind one authentication boundary is a wider
audit surface than the pre-release pin.

Replacing `openidconnect` solely to remove the split remains out of proportion
to a developer-surface concern; that judgement is unchanged.

## Related

- `backend/src/auth/oidc.rs`: the transport and the only module naming the
  adapter.
- `rust.just`: the `dependency-graph` recipe asserting the resolved outcome.

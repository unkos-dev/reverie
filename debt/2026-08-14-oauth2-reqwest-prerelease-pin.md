---
severity: low
surfaces: [developer]
adopted: 2026-09-29
adopted-because: the upstream reqwest 0.13 adapter for oauth2 is only available as a pre-release
lift-when-class: dep-unblocks
lift-when: oauth2-reqwest publishes a stable release compatible with the oauth2 and reqwest majors Reverie resolves
---

# The OIDC transport adapter is pinned to a pre-release

`openidconnect` uses `oauth2`, whose bundled HTTP client depends on reqwest 0.12. Reverie's shared OIDC transport uses
reqwest 0.13 through the upstream `oauth2-reqwest` adapter. The adapter is only available as `0.1.0-alpha.3`, so its
version is exact-pinned.

The adapter converts requests and responses on a credential-bearing authentication path. Each pin update requires review
of those conversions and the transport tests. Its API is private to `auth::oidc`, and `cargo deny` applies the project's
advisory, licence and source policy.

The pin can be lifted when a stable compatible adapter is published. Copying the adapter into Reverie would make the
project maintain that conversion code; retaining two reqwest majors would keep two HTTP and TLS stacks behind the
authentication boundary.

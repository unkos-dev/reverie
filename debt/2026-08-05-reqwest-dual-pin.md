---
severity: low
surfaces: [developer]
adopted: 2026-05-29
adopted-because: openidconnect 4 pulls oauth2 5, whose bundled reqwest feature pins reqwest 0.12 alongside the tree's reqwest 0.13
lift-when-class: internal-refactor
lift-when: a first-party `AsyncHttpClient` implementation over reqwest 0.13 replaces oauth2's bundled reqwest feature, OR upstream ships an oauth2-reqwest release supporting reqwest 0.13
---

# Two reqwest majors in the dependency tree

## Constraint

Reverie depends on `reqwest 0.13` directly. The OIDC login path pulls a
second copy: `openidconnect 4` depends on `oauth2 5`, whose `reqwest`
feature pins `reqwest 0.12`. Both majors therefore build into every
binary.

The split crate that would resolve this upstream, `oauth2-reqwest`, has
been stuck at `0.1.0-alpha.3` since February 2026. This residue is what
remained after the direct reqwest 0.13 bump (PR #149) landed everywhere
it could reach.

## Why this isn't the right shape

The cost is developer-surface rather than behavioural: a larger binary,
two HTTP stacks with independent TLS configuration to reason about, and
a wider audit surface than the tree needs. Nothing is incorrect, and no
security control depends on removing it.

## Preferred fix: bring your own client

This does not require forking `openidconnect` or `oauth2`, nor swapping
either crate. The reqwest pin lives in `oauth2`'s optional `reqwest`
feature, not in its core. As of `oauth2 5.0.0` the crate exposes an
`AsyncHttpClient` trait with a single method,
`call(HttpRequest) -> Future<Result<HttpResponse>>`, and a blanket impl
for any `Fn(HttpRequest) -> Future`. Upstream's own `oauth2-reqwest`
adapter is about 70 lines in one file; that is the entire reqwest
integration.

So the clean path keeps `openidconnect` as-is and only stops using its
bundled client:

1. Implement the `AsyncHttpClient` adapter over reqwest 0.13. Reverie
   already owns a hardened reqwest client and SSRF resolver in
   `backend/src/services/enrichment/http.rs`; wrapping that makes the
   OIDC path inherit the same SSRF guard, which the bundled client does
   not have.
2. Set `oauth2` and `openidconnect` to `default-features = false` and
   drop the `reqwest` feature.
3. Pass the adapter to `openidconnect`'s async request methods in
   `backend/src/auth/oidc.rs` for discovery and token exchange.
4. Rewire the OIDC mock client, which is already wiremock-based.

Effort: roughly half a day to a day. It removes `reqwest 0.12` from the
tree entirely, with an ongoing cost near zero (about 70 first-party
lines, no fork liability). The OAuth2 and OIDC state machines are
untouched; only which HTTP client `oauth2` calls changes.

This is Tier-2 auth-path code under `docs/security/codeguard/`, so it
needs security review rather than a casual edit. It is discretionary
work: a good candidate to absorb the next time the auth subsystem is
open, not standalone work.

## Evaluated alternatives if openidconnect goes unmaintained

The lift condition above is first-party work and is not blocked on
upstream. These were surveyed in case `openidconnect` stalls entirely
and the whole crate has to be replaced, so a future reader does not
cold-research it.

As of 2026-05-29 the `ramosbugs/openidconnect-rs` repository's last
commit was November 2025 and the latest stable release was 4.0.1, from
July 2025. Slow, not abandoned.

| Option                               | reqwest                    | Maintenance             | Switch cost                                                            |
| ------------------------------------ | -------------------------- | ----------------------- | ---------------------------------------------------------------------- |
| **stay** openidconnect 4.0.1         | dual 0.12 and 0.13         | slowing                 | none                                                                   |
| **openid** (kilork) 0.23             | **0.13 only**              | active-ish              | full auth rewrite                                                      |
| **mas-oidc-client** 0.11             | http-agnostic (BYO client) | strong (Element/Matrix) | rewrite plus off-label use                                             |
| jwt-authorizer / aliri / compact_jwt | varies                     | varies                  | largest: token validation only, hand-roll discovery and auth-code flow |

Conclusion: do not swap crates to clear this. Replacing the OIDC client
is a security-sensitive rewrite of `auth/oidc.rs` and the mock, which is
poor value against a low-severity developer-surface concession, and the
bring-your-own-client path above clears the same residue for a fraction
of the work. A swap becomes justified only if bundled with an
independently motivated auth refactor, or if `openidconnect` is
confirmed abandoned (no commits plus an unpatched advisory).

## Related

- `backend/src/auth/oidc.rs`: discovery and token exchange, the call
  sites that would take the adapter.
- `backend/src/services/enrichment/http.rs`: the SSRF-guarded reqwest
  0.13 client the adapter would wrap.

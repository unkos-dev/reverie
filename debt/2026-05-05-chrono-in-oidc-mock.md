---
severity: low
surfaces: [developer]
adopted: 2026-05-05
adopted-because: openidconnect v4 CoreIdTokenClaims::new public API requires chrono types at the call site; documented inline in backend/AGENTS.md and test_support.rs at adoption time
lift-when-class: dep-unblocks
lift-when: openidconnect v5 stable release decouples chrono types, OR migrate to alternative OIDC lib, OR introduce a wrap-and-convert layer at the test boundary
---

# chrono in OIDC test mock

## Constraint

The project standard for date/time handling is the `time` crate, not
`chrono`, recorded in `backend/AGENTS.md`. The standard is a
consistency choice: one date/time crate across the tree, and `time` is
the one with first-party `sqlx` and `utoipa` support. The scaffold
predated the decision, so the blueprint still mentions chrono; the
ratified posture is `time`.

The OIDC test mock (`backend/src/test_support.rs::oidc_mock`) builds
ID-token claims via `openidconnect::core::CoreIdTokenClaims::new`.
That constructor's public API in openidconnect v4 takes chrono types
(`chrono::DateTime<Utc>`) for issued-at / expiration / not-before.
The types are non-negotiable at the call site.

## Workaround

`backend/Cargo.toml` includes `chrono` as a dependency in `dev-dependencies`
(or feature-gated, depending on current state). `oidc_mock` constructs
chrono `DateTime<Utc>` values for the duration of the mock setup.
No first-party code outside `oidc_mock` touches chrono.

`backend/AGENTS.md` documents the carve-out, and
`backend/guards/chrono-allowlist.txt` is the enforced registry of
exempted call sites. The use is contained to the OIDC mock and must
not spread elsewhere.

## Why this isn't the right shape

Two crates for the same job is taxing for three reasons:

1. Cognitive overhead: contributors have to remember which crate
   applies where, and what conversions exist between them.
2. Compile time: chrono's deps add to the dev build.
3. The carve-out invites scope creep: every new test that touches
   OIDC claims has the same temptation.

This is a consistency cost, not a security one. An earlier version of
this entry argued that chrono widened the audit surface on CVE-history
grounds. That reasoning is obsolete: RUSTSEC-2020-0159 was fixed in
chrono 0.4.20, and the crate has been actively maintained since. The
dependency is carried in dev-dependencies for a single test call site,
and the reason to keep it contained is a coherent codebase, not risk.

## Lift conditions

Three independent paths can lift this debt:

1. **Upstream dep-unblock**: openidconnect v5 (or any future version)
   ships a constructor that takes generic time types or `time` crate
   types. Track the upstream issue tracker for openidconnect.
2. **Migrate OIDC lib**: switch to a different OIDC client crate that
   uses `time` natively. Substantial refactor, not motivated by this
   debt alone, but a future libauth refactor could absorb the change.
3. **Wrap-and-convert at the boundary**: write a thin local adapter
   (`oidc_mock::time_to_chrono`) that takes `time::OffsetDateTime` and
   returns the chrono type, contained to the mock. Lifts the
   "chrono touches first-party code" smell without removing the
   chrono dep. Cheaper than (1) or (2). Does not eliminate the
   dependency but isolates the conversion site to a single named
   function with a clear deletion target post-(1)/(2).

When any path completes:

1. Flip this entry to `status: lifted`, set `lifted`, set
   `superseded-by`.
2. Update `backend/AGENTS.md` to remove the carve-out (or narrow it
   if path 3 is taken).
3. Remove chrono from `Cargo.toml` if path 1 or 2 is taken.

## Related

- `backend/AGENTS.md`: carve-out documentation (would update on
  lift)
- `backend/src/test_support.rs::oidc_mock`: workaround site
- `backend/guards/chrono-allowlist.txt`: enforced exemption registry
- No tracked issue yet: file as part of any libauth refactor or
  when an upstream dep-unblock surfaces. Until then, this debt entry
  is the canonical record.

## Evaluated fallbacks if `openidconnect` goes unmaintained (2026-05-29)

This entry's lift conditions assume `openidconnect` upstream
eventually ships a v5 (or chrono-decoupled constructor). That
assumption has bus-factor risk: as of 2026-05-29 the
[ramosbugs/openidconnect-rs](https://github.com/ramosbugs/openidconnect-rs)
repo's last commit was Nov 2025, latest stable is **4.0.1** (Jul
2025), and the reqwest-0.13 split crate `oauth2-reqwest` has been
stuck at `0.1.0-alpha.3` since Feb 2026 with no movement. Not dead,
but slow. The same root pin also keeps `reqwest 0.12.28` in the tree
transitively (`openidconnect 4 → oauth2 5 → reqwest 0.12`), the
residue left by the closed reqwest 0.13 direct bump task (reqwest 0.13 direct bump,
PR #149).

Pre-evaluated escape hatches if upstream stalls indefinitely. None
strictly dominates, and is recorded so a future session doesn't
cold-research this:

| Option                               | reqwest                    | chrono                     | Maintenance             | Switch cost                                                           |
| ------------------------------------ | -------------------------- | -------------------------- | ----------------------- | --------------------------------------------------------------------- |
| **stay** openidconnect 4.0.1         | dual 0.12+0.13             | dev-dep only               | slowing                 | none                                                                  |
| **openid** (kilork) 0.23             | **0.13 only**              | **prod** (via biscuit 0.7) | active-ish              | full auth rewrite                                                     |
| **mas-oidc-client** 0.11             | http-agnostic (BYO client) | **prod** (via mas-jose)    | strong (Element/Matrix) | rewrite + off-label use                                               |
| jwt-authorizer / aliri / compact_jwt | —                          | varies                     | varies                  | largest — token-validation only, hand-roll discovery + auth-code flow |

Key trap: the only full-RP alternative that clears the reqwest-dual
residue (`openid`) drags **chrono into the production tree** via
`biscuit`, which is _strictly worse_ for this debt, today chrono is
dev-dep-only. `mas-oidc-client` is best-maintained but purpose-built
for the Matrix Authentication Service and also chrono-based.

Conclusion: do **not** pivot to clear this debt alone. Both this
chrono debt and the reqwest-dual residue are `severity: low,
surfaces: developer` (binary size + audit surface, not correctness or
a security hole); a security-sensitive auth-subsystem rewrite
(`auth/oidc.rs` + `backend.rs` + `oidc_mock`) is poor ROI against
low-severity cosmetic debt. A pivot becomes justified only if bundled
with an independently-motivated libauth refactor, OR if
`openidconnect` is confirmed abandoned (no commits + an unpatched
advisory). Until then, keep watching upstream.

### Preferred fix for the reqwest-dual residue: bring-your-own client (not a fork, not a crate swap)

The reqwest-0.12 residue does **not** require any of the crate-swap
options above, nor forking `openidconnect`/`oauth2`. The reqwest pin
lives in `oauth2`'s optional `reqwest` _feature_, not in its core. As
of `oauth2 5.0.0` the crate exposes an `AsyncHttpClient` trait, a
single method `call(HttpRequest) -> Future<Result<HttpResponse>>`,
with a blanket impl for any `Fn(HttpRequest) -> Future`. Upstream's
own `oauth2-reqwest` adapter is **70 LOC in one file**; that is the
entire reqwest integration.

So the clean path keeps `openidconnect` exactly as-is and only stops
using its bundled reqwest:

1. Vendor / re-implement the ~70 LOC `AsyncHttpClient` impl over
   **reqwest 0.13**. Reverie already owns a hardened reqwest client +
   SSRF resolver in `backend/src/services/enrichment/http.rs`: wrap
   that, so the OIDC path inherits the same SSRF guard.
2. `Cargo.toml`: set `oauth2` / `openidconnect` to
   `default-features = false` and drop the `reqwest` feature.
3. Pass the adapter to `openidconnect`'s `*_async` request methods in
   `backend/src/auth/oidc.rs` (discovery + token exchange).
4. Fix `oidc_mock` client wiring (already wiremock-based).

**Effort: ~0.5–1 day.** Removes `reqwest 0.12.28` from the tree
entirely; ongoing cost ≈ nil (≈70 first-party lines, no fork
liability). The OAuth2/OIDC state machine is untouched: only _which_
HTTP client `oauth2` calls changes.

Scope caveats:

- This clears **only** the reqwest-dual residue (leftover from the reqwest 0.13 direct bump task). It
  does **not** lift this chrono debt; chrono is forced by
  `CoreIdTokenClaims::new`'s signature, orthogonal to the HTTP client.
- It is still Tier-2 auth-path code (`docs/security/codeguard/`), needs
  security review, not a casual edit.
- Discretionary, not urgent: residue is `severity: low`. Good
  "knock out next time we're in `auth/`" candidate rather than
  standalone work.

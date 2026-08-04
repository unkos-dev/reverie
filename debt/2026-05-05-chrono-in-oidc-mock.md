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
consistency choice: one date/time crate across the tree. Both crates
are supported by `sqlx` and `utoipa`, so availability is not the
differentiator; `time` is simply the one wired up here, and
`backend/Cargo.toml` leaves the `chrono` features off deliberately.
The scaffold predated the decision, so the blueprint still mentions
chrono; the ratified posture is `time`.

The OIDC test mock (`backend/src/test_support.rs::oidc_mock`) builds
ID-token claims via `openidconnect::core::CoreIdTokenClaims::new`.
That constructor's public API in openidconnect v4 takes chrono types
(`chrono::DateTime<Utc>`) for issued-at / expiration / not-before.
The types are non-negotiable at the call site.

## Workaround

`backend/Cargo.toml` declares `chrono` under `[dev-dependencies]`.
`oidc_mock` constructs chrono `DateTime<Utc>` values for the duration
of the mock setup. No first-party code outside `oidc_mock` touches
chrono.

Three separate facts are easy to conflate here, so state them apart:

1. **First-party usage** is confined to `oidc_mock`. This is what the
   debt is about.
2. **The direct declaration** is a dev-dependency, so no first-party
   production code compiles against chrono.
3. **The production graph still contains chrono** transitively, through
   `jwks_client_rs`, and through `oauth2` under `openidconnect`. Verify
   with `cargo tree --locked -e normal -i chrono`. Removing the
   dev-dependency would not remove chrono from the built binary.

`backend/AGENTS.md` documents the carve-out and
`backend/guards/chrono-allowlist.txt` registers it. Be precise about
what the guard covers, because it is narrower than it looks on two
axes. It greps `backend/src/` for `use chrono` and `extern crate
chrono` only, so a fully-qualified `chrono::` path is caught nowhere,
allowlisted or not. And the allowlist entry is the bare path
`backend/src/test_support.rs:`, matched as a substring against the
`grep -rn` output, so the exemption covers the whole file rather than
the `oidc_mock` function.

Keeping the use inside `oidc_mock` and in import form is therefore a
rule the guard cannot fully enforce, not a rule the guard relaxes.
The blast radius is bounded: `test_support` is `#[cfg(test)]`-gated
(`backend/src/lib.rs:37`), so anything the guard misses is still test
code that cannot reach a production build.

## Why this isn't the right shape

Two crates for the same job is taxing for three reasons:

1. Cognitive overhead: contributors have to remember which crate
   applies where, and what conversions exist between them.
2. Compile time: chrono's deps add to the dev build.
3. The carve-out invites scope creep: every new test that touches
   OIDC claims has the same temptation.

This is a consistency cost, not a security one. An earlier version of
this entry argued that chrono widened the audit surface on CVE-history
grounds. That reasoning does not hold on either count. RUSTSEC-2020-0159
was fixed in chrono 0.4.20 and the crate has been actively maintained
since, so its presence is not a risk to weigh; and lifting this debt
would not shrink the audit surface anyway, because chrono stays in the
production graph transitively either way. The reason to keep the use
contained is a coherent codebase.

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

Only paths 1 and 2 resolve this debt. Path 3 narrows it: first-party
chrono usage remains, `[dev-dependencies]` keeps chrono, and the
allowlist entry stays, so the wrong shape is still in the tree and
`README.md`'s hard rules keep the entry active.

When path 1 or 2 completes and the proper fix has shipped, purge rather
than archive, per those hard rules:

1. Remove the carve-out from `backend/AGENTS.md`.
2. Remove chrono from `[dev-dependencies]` and drop the
   `backend/guards/chrono-allowlist.txt` entry.
3. Delete this file and its line in `README.md`, leaving no tombstone.
   The purge commit message names the resolving PR.

When path 3 completes, amend rather than purge: narrow the carve-out in
`backend/AGENTS.md` to the adapter function, and update this entry's
`lift-when` to the remaining paths.

An unblocked path whose fix has not shipped leaves this entry active.

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

| Option                               | reqwest                    | chrono                               | Maintenance             | Switch cost                                                           |
| ------------------------------------ | -------------------------- | ------------------------------------ | ----------------------- | --------------------------------------------------------------------- |
| **stay** openidconnect 4.0.1         | dual 0.12+0.13             | dev-dep + already transitive in prod | slowing                 | none                                                                  |
| **openid** (kilork) 0.23             | **0.13 only**              | **prod** (via biscuit 0.7)           | active-ish              | full auth rewrite                                                     |
| **mas-oidc-client** 0.11             | http-agnostic (BYO client) | **prod** (via mas-jose)              | strong (Element/Matrix) | rewrite + off-label use                                               |
| jwt-authorizer / aliri / compact_jwt | —                          | varies                               | varies                  | largest — token-validation only, hand-roll discovery + auth-code flow |

Key trap: the only full-RP alternative that clears the reqwest-dual
residue (`openid`) puts chrono on a **first-party production call
path** via `biscuit`, which is worse for this debt than today's
position. Today chrono is already in the production graph
transitively, but no first-party production code compiles against it.
`mas-oidc-client` is best-maintained but purpose-built for the Matrix
Authentication Service and also chrono-based.

Conclusion: do **not** pivot to clear this debt alone. Both this
chrono debt and the reqwest-dual residue are `severity: low,
surfaces: developer` (binary size and codebase coherence, not
correctness or a security hole); a security-sensitive auth-subsystem rewrite
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

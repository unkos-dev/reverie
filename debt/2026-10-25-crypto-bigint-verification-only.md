---
severity: high
surfaces: [security, server-operator, ci]
adopted: 2026-10-25
adopted-because: accepted risk for assessed OIDC signature-verification-only use while the compatible dependency graph has no patched release
lift-when-class: dep-unblocks
lift-when: the resolved dependency graph contains no crypto-bigint version affected by RUSTSEC-2026-0387
---

# crypto-bigint timing risk in OIDC signature verification

`backend/deny.toml` and `backend/osv-scanner.toml` accept
[RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html) for the assessed production use of
`crypto-bigint` in OIDC signature verification. This exception accepts residual risk; it does not fix the vulnerability.

## Dependency constraint

`openidconnect` 4.0.1 brings `p256` 0.13.2 and `p384` 0.13.1 through `elliptic-curve` 0.13.8, which requires
`crypto-bigint` 0.5. The resolved version is 0.5.5. The advisory's patched range starts at 0.7.6, outside that
requirement, so a lockfile update cannot apply the fix. The separate `ctutils` 0.4.4 update does not repair this copy of
`crypto-bigint`.

As assessed on 2026-10-25, [openidconnect PR #244](https://github.com/ramosbugs/openidconnect-rs/pull/244) is open and
proposes a broad cryptographic dependency upgrade. It is not a supported release that resolves this constraint.

## Assessed use and residual risk

The production callback in `backend/src/routes/auth.rs` exchanges an authorisation code using PKCE before
`backend/src/auth/oidc.rs` verifies the provider's ID token against JWKS public keys. ECDSA verification reaches
arithmetic on public key coordinates, signature values `r` and `s`, and the token digest. Source inspection found no
production private signing or ECDH caller using this dependency. HMAC verification with the client secret follows a
separate path.

Verification-only use does not make the affected arithmetic unreachable or prove that every operand is public. ID token
claims may be confidential, and this assessment does not rule out timing leakage on the deployed compiler and CPU.
Whether an attacker can exploit this in practice remains unresolved.

## Review and removal

Review the exception by **2026-11-08**. This is a documented review deadline, not an automatically enforced expiry in
either scanner configuration.

Reassess immediately if signing, ECDH or other secret-integer use becomes reachable, or if exploit evidence contradicts
the assessment. Remove both scanner exceptions, this entry and its README link when the resolved dependency graph
contains no `crypto-bigint` version affected by RUSTSEC-2026-0387.

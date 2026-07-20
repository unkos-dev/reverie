---
severity: low
surfaces: [security, ci]
adopted: 2026-07-20
adopted-because: Snyk supports Cargo only through `snyk sbom test`, which creates no monitored target
lift-when-class: dep-unblocks
lift-when: Snyk supports Cargo natively in `snyk test` and `snyk monitor`, so `backend/Cargo.lock` scans from source and appears as a monitored target with drift history
---

# Snyk sees no Rust dependencies

The Snyk advisory workflow covers three of the project's four dependency
surfaces. Snyk Code analyzes the Rust and TypeScript sources, Snyk Open
Source reads the npm lockfile, and Snyk Container reads the published
runtime image. Rust dependencies are absent.

## Why

Snyk does not support Cargo in `snyk test` or `snyk monitor`. Its only
Rust path is `snyk sbom test`, which consumes a CycloneDX or SPDX
document produced by a third-party generator. That path is unusable for
this workflow's purpose on three counts:

- It creates no monitored target, so there is no dashboard entry, no
  drift history, and no remediation trend. Snyk's SBOM API confirms the
  asymmetry: SBOM tests are ephemeral jobs, and the only project-linked
  endpoint generates a document out of a target Snyk already monitors.
- It emits no SARIF, so findings would land in job logs rather than the
  code-scanning dashboard where every other lane reports.
- It would require a fourth scanner binary pinned in CI purely to
  re-serialize a lockfile the workflow already has in its checkout.

## Why this is low severity

`cargo-deny` scans `backend/Cargo.lock` directly on every PR and gates
on advisories and licenses. Reading the lockfile is strictly higher
fidelity than reading an SBOM derived from it, so the Rust surface is
not unscanned; it is scanned by a different tool, and Snyk would be a
second opinion rather than the only opinion.

The debt is the asymmetry itself. Snyk's value in this workstream is a
cross-checked baseline across the whole dependency surface, and one
quarter of that surface cannot participate. Any comparison of Snyk
against the incumbent scanners has to carry the caveat, and a future
decision to promote Snyk to a gate cannot cover Rust.

## Do not confuse with the image SBOM gap

Reverie publishes an SPDX attestation with each image. That document
describes the runtime image filesystem, and the crates are compiled into
the backend binary rather than installed as packages, so it does not
list them either. That gap is about what downstream self-hosters can
answer from a published artifact, and it is fixable here (for example by
building with `cargo auditable` so the dependency list is recoverable
from the binary). This entry is about Snyk's ingestion path and is not
fixable here.

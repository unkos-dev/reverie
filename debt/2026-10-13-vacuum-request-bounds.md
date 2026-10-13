---
severity: medium
surfaces: [developer, security, ci]
adopted: 2026-10-13
adopted-because: "PR #1404 adopts Vacuum 0.32.0 with a temporary baseline for existing request bounds."
lift-when-class: internal-refactor
lift-when: All temporary string-limit and array-limit paths are removed after verified bounds or individually justified contract decisions ship.
---

# Request bounds exposed by Vacuum 0.32.0

Vacuum now detects request schemas previously missed by its OWASP rules. The temporary baseline in
[.vacuum-ignore.yaml](../.vacuum-ignore.yaml) covers 152 string-limit and 18 array-limit findings on adoption. Both
rules remain enabled; this defers schema corrections and runtime hardening.

- Existing runtime bounds: publish accurate schema limits or enums, then remove the corresponding paths.
- Normalised inputs and missing bounds: agree raw-input limits, enforce them with boundary tests, and remove the paths.
- Contract-dependent inputs: decide each contract separately, then replace its temporary path with a verified bound or a
  documented exception.

Prioritise shared components, identifiers, cursors and pagination. An ignored component property stays ignored for every
endpoint that references it, including future endpoints. Exact paths detect new properties but do not prevent this
inherited exception. Parameter paths also use array positions, so parameter reordering requires reviewing the baseline.
OIDC authorisation codes need a local-limit decision; they are not a permanent exception merely because a provider
defines them.

Delete each baseline path with its remedy and purge this entry when no temporary paths remain.

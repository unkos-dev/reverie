---
type: ADR
profile-version: 1
id: "REV-ADR-0055"
title: "Production analysis and credential test boundaries"
status: "accepted"
recorded-on: "2026-10-09"
decided-on: "2026-10-08"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0042"
---

# Production analysis and credential test boundaries

## Context and problem statement

Credential handling needs different rules for operational values, isolated test credentials and literals that exercise
credential handling. Operational credentials come from external direct variables or mounted files. Isolated tests use
generated credentials. Credential-handling literals are test data, while production defaults and disclosure remain
security findings.

Production SAST and dashboard ingestion have different boundaries. Test-only CodeQL code produces hard-coded credential
findings without providing production coverage. Snyk distinguishes test findings through rule IDs and reports Debian
base-layer vulnerabilities for which the scanned release has no fix. These findings need an explicit analysis or
ingestion decision rather than blanket exclusions of mixed files.

## Decision drivers

- Keep production code, mixed production/test files and setup tools analysed.
- Prove each CodeQL boundary with a detected production control and detected test controls before exclusion.
- Preserve secret scanning across the repository.
- Keep withheld Snyk results available for review and restore findings when a fix becomes available.

## Considered options

- Analyse all test code and dismiss individual alerts.
- Exclude whole mixed files or base-image layers from scans.
- Exclude proven CodeQL test-only code and retain Snyk analysis with bounded SARIF ingestion filters.

## Decision outcome

Chosen option: **exclude proven CodeQL test-only code and retain Snyk analysis with bounded SARIF ingestion filters**,
because it separates production analysis from test data without hiding production code or fixable dependency findings.

CodeQL Rust extraction disables the `test` configuration through `cargo_cfg_overrides=-test` and excludes
`backend/tests` through its path configuration. The extractor environment override belongs to the Rust job alone.
Production modules remain analysed, including production code in files that also contain `#[cfg(test)]` modules. A
filename such as `tests.rs` does not establish an exclusion boundary. Baseline and excluded controls exercise the same
full Rust suite and source contents; the production control remains in SARIF and the extracted database while both test
controls disappear. Raw proof SARIF is retained privately for the decision's evidence.

Snyk Code analysis stays unchanged. Dashboard ingestion withholds only exact rule IDs registered in
`.github/snyk-code-test-rule-allowlist.txt`, independently of file path. Ordinary rule IDs remain visible in test files.
An unregistered `/test` ID fails the job. The `snyk-code-sarif-unfiltered` artifact retains raw results for 14 days,
including when the filter refuses a new test rule.

Snyk Container withholds a finding only when its rule ID uses the `SNYK-DEBIAN<n>-` distro namespace and its remediation
text states that no fixed version exists for that same Debian release. Application-layer findings without a fix remain
visible, as do findings offering a fix for the scanned release despite an older release having none. The predicate runs
against each scan, so a newly available fix restores the finding. Unclassifiable results remain visible. Raw results
stay in `snyk-container-sarif-unfiltered` for 14 days and withheld counts appear in the step summary. Withholding an
unfixable base-OS CVE accepts risk; it makes no VEX `not_affected` assertion. The SBOM still lists all packages.

Snyk Open Source scans all projects with development dependencies. Its severity sanitiser removes invalid non-numeric
CVSS properties from licence findings without dropping findings. Main-branch monitoring refreshes the dependency
baseline with `--target-reference=main`.

### Consequences

- Positive: production analysis and secret scanning retain their coverage.
- Positive: Snyk ingestion predicates apply to current results and retain raw output for review.
- Negative: CodeQL gives up analysis of `#[cfg(test)]` modules and integration tests, including traces originating
  there.
- Negative: allowlisted Snyk test-rule findings are withheld regardless of individual validity; raw artifacts require
  review within their retention window.
- Negative: the CodeQL Rust extractor is beta, so changes to its options or test boundaries require renewed proof.
- Negative: the container predicate depends on remediation wording; changed wording restores alerts until reviewed.

---
type: ADR
profile-version: 1
id: "REV-ADR-0042"
title: "Code-scanning ingestion policy: ingest what is actionable"
status: "accepted"
recorded-on: "2026-09-06"
decided-on: "2026-07-23"
decision-makers:
  - "John Unkovich"
---

# Code-scanning ingestion policy: ingest what is actionable

## Context and problem statement

Adding Snyk alongside the incumbent scanners took the open code-scanning alert count to 155. Six were surfaced as
critical, and none of the six was a vulnerability: four were Debian base-layer CVEs that Snyk itself rates low and
GitHub relabelled critical by reading the raw NVD score out of the SARIF `security-severity` property, and two were
CodeQL hard-coded-secret findings on a randomly filled buffer and on test fixtures.

Of the remainder, 88 are Snyk Container findings and every one of them reports that no fixed version exists in
Debian 13, and 47 are Snyk Code findings carrying a `<rule>/test` rule ID, the variant Snyk emits when it places a
finding in test code.

That leaves a dashboard where roughly one alert in ten can be acted on. A queue at that signal-to-noise ratio is not
triaged, it is ignored, and an ignored queue is worse than a smaller honest one because it hides the findings that
matter.

The question is not whether to reduce the noise but where to cut. Scan coverage and dashboard ingestion are
separable, and conflating them is how suppression turns into blindness.

## Decision drivers

- A suppression must never reduce what is analysed, only what opens an alert.
- Nothing fixable may be hidden, and the mechanism must make that verifiable by reading it rather than by trusting a
  claim.
- Suppression scope must not widen on its own as scanners add rules or as the base image changes.
- Withheld findings must stay observable to a reviewer.
- No standing list that silently goes stale.

## Considered options

- Exclude test files and base-image layers from the scans themselves (`.snyk` `exclude:` block,
  `--exclude-base-image-vulns`).
- Dismiss each alert in the GitHub UI as it arrives.
- Filter the uploaded SARIF, leaving the scans untouched.
- Carry a `.snyk` ignore list of specific vulnerability IDs with expiry dates, plus a drift gate that re-scans
  unfiltered and fails when an ignored ID becomes fixable.

## Decision outcome

Chosen option: **filter the uploaded SARIF, leaving the scans untouched**, because it is the only option that
reduces the alert queue without reducing analysis, and because both filters can be written as predicates over the
current scan rather than as lists that need maintaining.

Two predicates, one per lane.

**Snyk Code** withholds results whose rule ID appears in `.github/snyk-code-test-rule-allowlist.txt`. The filter
keys on rule ID and never on file path. Snyk emits a distinct `<rule>/test` ID for findings it places in test code,
so withholding those IDs cannot suppress a rule class that has no `/test` form: a genuine defect in a test file
arrives under its ordinary rule ID and still opens an alert. A `/test` rule ID absent from the allowlist fails the
job, so the scope only widens by a reviewed commit.

**Snyk Container** withholds a result only when its rule ID is in the distro namespace (`SNYK-DEBIAN<n>-`) and that
rule's remediation text states no fixed version exists for the release named in that same rule ID. An
application-layer dependency with no upstream fix is still actionable and stays, and so does an advisory that
mentions an older release having no fix while offering an upgrade for the release being scanned. The predicate runs
against each scan, so when Debian ships a fix the text changes, the finding stops matching, and the alert returns
unaided.

Rejecting the `.snyk` ignore list is the substantive call. It would have required a `--policy-path` flag on both the
`test` and `monitor` invocations, 88 entries with renewable expiry dates, and a second unfiltered scan plus a diff to
detect an entry becoming fixable. The dynamic predicate gets the same guarantee for free, because an entry that
becomes fixable stops matching. The cost is the loss of a per-CVE audit trail, which is a poor trade for base-OS
CVEs: the entries would all carry the same reason, and renewing 88 expiry dates on a schedule is a ritual, not a
judgement.

A note on vocabulary, since these findings reach an SBOM consumer: withholding an unfixable base-OS CVE is a risk
acceptance, not a CISA VEX `not_affected` assertion. "No fix is available" is not one of the five `not_affected`
justifications, and this decision does not claim it is. Some individual findings would qualify (a 32-bit-only
overflow against an arm64 image is `vulnerable_code_not_in_execute_path`), but the filter as a class does not, and
dressing it up as one would be the dishonest version of this decision.

### Consequences

- Positive: the dashboard becomes a queue someone can work rather than an inventory nobody reads.
- Positive: analysis coverage is untouched: Snyk Code still walks every test file, so a taint trace running from a
  test helper into production code is still found.
- Positive: neither filter holds state that can drift out of date.
- Positive: both filters fail closed. An unclassifiable finding is ingested, so a reworded remediation section
  produces a burst of alerts rather than silence.
- Negative: an unfixable base-OS CVE is now visible only in the Snyk monitor baseline, the retained SARIF artifact,
  and the step summary, not in the code-scanning dashboard.
- Positive: the published SBOM continues to list every base-layer package regardless of what the dashboard ingests.

## More information

Revisit when Snyk Container moves from advisory to a gate, when the runtime base image changes distribution, or if
Snyk begins emitting a structured fix-availability field, which would replace the remediation prose the container
predicate currently reads.

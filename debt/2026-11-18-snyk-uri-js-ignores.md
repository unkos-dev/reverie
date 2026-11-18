---
severity: low
surfaces: [developer, ci, security]
adopted: 2026-11-18
adopted-because: ESLint 10.15.0 resolves uri-js 4.4.1 through its JSON Schema validator, and no uri-js release fixes the three advisories Snyk reports against it
lift-when-class: dep-unblocks
lift-when: the ESLint the schema lint runs on no longer resolves uri-js 4.4.1, and snyk test --all-projects --dev reports no uri-js issues with the three ignores removed
---

# Snyk ignores for ESLint's URI parser

`.snyk` ignores SNYK-JS-URIJS-19963963, SNYK-JS-URIJS-19963961 and SNYK-JS-URIJS-20341609 on the `eslint > ajv > uri-js`
path only. The advisories affect `uri-js` 4.4.1, its last release, and Snyk lists no upgrade or patch for them. The
latest ESLint still declares `ajv` `^6.14.0`.

ESLint runs only the schema lint, as a root development dependency. Its `ajv` compiles the rule-option schemas that
ESLint and its plugins ship, so no input from outside the repository reaches `uri-js`. The CI scan still reports a
`uri-js` copy on any other path.

The Snyk pull request check does not apply the `.snyk` ignores, so the `unkos-dev/reverie:package.json` Snyk project
ignores the same three advisories on every path, as not vulnerable.

Every ignore expires on 2027-05-18. Remove the ignores when ESLint no longer resolves `uri-js` 4.4.1. Delete `.snyk`, or
its three entries, and the three project ignores, together with this entry and its README link.

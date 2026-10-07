---
type: ADR
profile-version: 1
id: "REV-ADR-0053"
title: "Typed credentials and externally supplied authentication material"
status: "proposed"
recorded-on: "2026-10-07"
decision-makers:
  - "John Unkovich"
---

# Typed credentials and externally supplied authentication material

## Context and problem statement

Configuration diagnostics can expose authentication material even when consumers never log it deliberately. Derived
Debug renders plain strings, and parsing or validation can reproduce a supplied value before a secret wrapper exists.
The configuration schema and reference also publish defaults, so changing representation cannot remove their explicit
empty or null credential defaults.

Credential supply has three distinct purposes: operators supply deployment credentials, provisioning owns development
and disposable test credentials, and credential-handling tests use deliberate literals to exercise the boundary.
Treating these alike either leaves usable operational defaults or removes meaningful test inputs.

## Decision drivers

- Prevent credential disclosure through configuration diagnostics without hiding the setting operators need to fix.
- Preserve the declarative configuration stack, external setting names and required or optional status.
- Make plaintext access explicit at connection and external-client boundaries.
- Separate disposable test ownership from persistent development state and production credentials.

## Considered options

- Plain strings with handwritten Debug and error scrubbing.
- Typed secrets with error scrubbing and externally supplied or provisioning-owned credentials.
- A replacement configuration and secret-management subsystem.

## Decision outcome

Chosen option: **Typed secrets with error scrubbing and externally supplied or provisioning-owned credentials**, because
it protects ordinary diagnostic formatting while retaining the existing configuration and consumer contracts.

The six credential-bearing configuration fields use `secrecy` 0.10.3 with Serde support. Required fields retain
`SecretString`; optional fields retain `Option<SecretString>`. Explicit Schemars annotations preserve string or nullable
string schema types and empty or null defaults. Consumers expose plaintext only where their existing APIs require it.
One credential classification covers parsing and validation: wrappers cannot prevent a parser from quoting a value
before constructing them. Credential failures therefore receive a fixed value-free reason, including struct-level
validation failures that name a variable explicitly. Non-secret diagnostics retain their reasons.

Deployment credentials come from operators. Disposable test provisioning generates credentials and owns its cluster and
child execution; SQLx retains per-test databases and migrations. Persistent development provisioning generates
credentials once and reuses them across restarts. Both TCP and Unix sockets use SCRAM, with no fallback to the
persistent development database for disposable runs. Deliberate credential-handling literals remain test data.

Credential file supply rejects simultaneous `VAR` and `VAR_FILE` sources, reads UTF-8, removes one final LF or CRLF,
preserves other content, and reports failed reads without values. This supply decision covers the six configuration
credentials and the separate migrate-only reader; it does not expand bootstrap credential storage.

### Consequences

- Positive: Config clones retain diagnostic redaction, and plaintext access is visible at consumers.
- Positive: parsing and validation failures identify settings without retaining raw credential messages or parameters.
- Positive: generated defaults remain reviewable without publishing authentication material.
- Negative: consumers and test configurations must accommodate secret types, with explicit copies where external APIs
  require owned strings.
- Negative: wrappers do not protect process environment values, exposed strings or downstream errors automatically.
- Negative: provisioning and credential-file supply require separate lifecycle and compatibility verification.

## Pros and cons of the options

### Plain strings with handwritten Debug and error scrubbing

- Positive: fewer consumer changes and no representation dependency.
- Negative: handwritten formatting must remain complete whenever credential fields change, and other derived
  representations can expose the same values.

### Typed secrets with error scrubbing and externally supplied or provisioning-owned credentials

- Positive: ordinary Debug redaction follows the field type while existing libraries retain their interfaces.
- Negative: schema defaults need explicit annotations, and parsing still needs independent scrubbing.

### A replacement configuration and secret-management subsystem

- Positive: one subsystem could centralise supply, reflection and diagnostics.
- Negative: it replaces working configuration contracts and adds an ownership surface beyond the credential boundary.

## More information

Delivered coverage consists of typed configuration, parsing and validation scrubbing, explicit consumer access, and
six-field default and startup-output regressions. Generated provisioning credentials and credential-file readers are
separate implementation work; this record does not establish that they are delivered.

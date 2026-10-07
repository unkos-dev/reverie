---
type: ADR
profile-version: 1
id: "REV-ADR-0053"
title: "Typed credentials and externally supplied authentication material"
status: "accepted"
recorded-on: "2026-10-07"
decision-makers:
  - "John Unkovich"
---

# Typed credentials and externally supplied authentication material

## Context and problem statement

Configuration diagnostics can expose authentication material even when consumers never log it deliberately. Derived
Debug renders plain strings. Parsing can quote a supplied value before a secret wrapper exists; validator messages and
codes can expose it after wrapping. The configuration schema and reference also publish defaults, so changing
representation cannot remove their explicit empty or null credential defaults.

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

`secrecy` is chosen over handwritten diagnostic formatting so redaction follows the credential type. Error scrubbing
remains a separate boundary because parsing and validation can quote values independently of secret wrappers. Plaintext
access is explicit at consumers, while the existing configuration and published-default contracts remain unchanged.

Operators own deployment credentials. Provisioning owns development and disposable test credentials, keeping test
lifecycle ownership separate from persistent development state. Deliberate credential-handling literals remain test data
rather than operational credentials.

Environment variables and credential files are the selected supply mechanisms, avoiding an application-owned secret
store. The choice covers configuration credentials and the separate migrate-only reader without expanding bootstrap
credential storage.

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

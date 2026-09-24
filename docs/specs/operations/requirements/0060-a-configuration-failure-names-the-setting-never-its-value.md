---
type: REQ
profile-version: 1
id: "REV-REQ-0060"
title: "A configuration failure names the setting, never its value"
---

# A configuration failure names the setting, never its value

## Statement

WHEN Reverie refuses to start because a configuration setting is absent, unreadable, or invalid, the failure it reports
MUST NOT contain the value the environment supplied for a credential-carrying setting. A credential-carrying setting is
one whose value authenticates Reverie to something else: a database connection string, the identity provider's client
secret, or a metadata provider's API key.

## Rationale

A startup failure is the Reverie message an operator is most likely to move somewhere else: into a container log their
orchestrator retains, into an issue report, into a thread asking for help. The settings this obligation covers are the
ones that reach the database and the external metadata providers, so a failure that quoted one back hands a live
credential to everyone who reads that text afterwards. The operator carries that loss and has no way to notice it:
nothing in the shared text distinguishes a failure that quoted a secret from one that did not, and by the time it has
been shared the credential is already out.

## Acceptance criteria

- A decoding failure on a credential-carrying setting reports the setting's name and a fixed reason, with no fragment of
  the supplied value anywhere in the reported text. Checked by `secret_field_deser_error_has_no_value` in
  `backend/src/config/mod.rs`, which supplies a recognisable value and asserts its absence.
- A validation failure on a credential-carrying setting reports no more than a decoding failure does. Nothing on the
  validation path inspects the credential-carrying set, so satisfaction rests instead on no credential-carrying setting
  declaring a validation rule; that is determined by reading the field declarations in `backend/src/config/mod.rs` and
  `backend/src/config/security.rs`. No automated check covers this criterion.

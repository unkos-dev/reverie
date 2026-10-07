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

- A decoding failure on each of the six credential-carrying settings reports its setting name and a fixed reason,
  without its supplied value in Display or Debug. Checked by `secret_field_deser_error_has_no_value` in
  `backend/src/config/mod.rs` and the raw process-output assertions in
  `ingestion_startup_diagnostics_redact_credentials` in `backend/tests/ingestion_startup.rs`.
- Field-level and struct-level validation failures on each credential-carrying setting report its name without
  credential-bearing messages, codes or parameters in Display or Debug. Checked by
  `secret_field_validation_error_has_no_value` in `backend/src/config/mod.rs`.
- Nested and list validation errors preserve aggregation, setting names and useful non-secret reasons while omitting
  credential markers. Checked by `nested_validation_errors_preserve_multiple_and_non_secret_reasons` and
  `non_secret_validation_error_retains_reason` in `backend/src/config/mod.rs`.

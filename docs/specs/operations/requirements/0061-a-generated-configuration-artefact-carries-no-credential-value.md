---
type: REQ
profile-version: 1
id: "REV-REQ-0061"
title: "A generated configuration artefact carries no credential value"
governed-by:
  - "REV-ADR-0023"
---

# A generated configuration artefact carries no credential value

## Statement

WHEN Reverie generates its configuration reference or its configuration JSON Schema, the generated artefact MUST NOT
carry a credential-carrying setting's value as that setting's default. A credential-carrying setting is one whose value
authenticates Reverie to something else: a database connection string, the identity provider's client secret, or a
metadata provider's API key.

## Rationale

Both artefacts are committed and published: the JSON Schema as `backend/config.schema.json`, the reference as a page on
the documentation site. A default rendered from a populated field is therefore a credential in Git history and on a
public page, recoverable long after the field itself was changed. The generators read the same declarations the loader
reads, so a developer who gives a credential-carrying field a non-empty default publishes it at the next regeneration,
and no step between those two events looks like disclosure to a reviewer.

## Acceptance criteria

- `backend/config.schema.json`, as committed, emits for each credential-carrying setting a default that is either an
  empty string or null: an empty string for `database_url`, `ingestion_database_url` and `oidc_client_secret`, null for
  `migration_database_url`, `googlebooks_api_key` and `hardcover_api_token`.
- The committed configuration reference leaves the default column empty for every one of those six settings, describing
  each by name, type and whether it is required.
- `config_schema_has_no_secret_default_values` in `backend/src/config/mod.rs` asserts the schema criterion for three of
  the six: `oidc_client_secret`, `googlebooks_api_key` and `hardcover_api_token`. The other three satisfy it through the
  empty string their `Default` implementation sets, which no assertion covers, so a change to one of those three
  defaults is caught by inspecting the regenerated artefact and by nothing else.

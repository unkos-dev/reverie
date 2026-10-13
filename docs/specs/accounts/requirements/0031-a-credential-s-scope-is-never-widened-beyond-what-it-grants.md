---
type: REQ
profile-version: 1
id: "REV-REQ-0031"
title: "A credential's scope is never widened beyond what it grants"
governed-by:
  - "REV-ADR-0056"
---

# A credential's scope is never widened beyond what it grants

## Statement

A credential that resolves to no scope MUST be refused rather than granted the account's role-derived scope set. A
credential's effective scope set MUST NOT be wider than what the credential itself grants.

## Rationale

Substituting a broader scope set for a credential that grants none defeats the purpose of a scoped credential. Scope
limits the capability delegated to a token, as described by
[RFC 6749 §3.3](https://www.rfc-editor.org/rfc/rfc6749#section-3.3). Falling back to the account's role-derived set
would violate least privilege. This obligation binds credential resolution; the operation-level scope gates are separate
obligations under REV-REQ-0005 and REV-REQ-0006.

## Acceptance criteria

- A token carrying no scope is refused at every operation. Checked by `scopeless_token_rejected_by_every_op` in
  `backend/src/authz_matrix.rs`.
- A token carrying only read scope cannot perform a mutation, even when its owner is an administrator. Checked by
  `read_token_blocked_from_every_mutation` in `backend/src/authz_matrix.rs`.
- A token carrying only write scope cannot perform an admin operation. Checked by
  `write_token_blocked_from_every_admin_op` in `backend/src/authz_matrix.rs`.
- A session resolves to the account's full role-derived scope set; it carries no separate scope grant to narrow.
  Verified by inspection of `CurrentUser::from_request_parts` in `backend/src/auth/middleware.rs`.

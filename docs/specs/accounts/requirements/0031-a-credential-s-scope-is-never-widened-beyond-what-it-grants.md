---
type: REQ
profile-version: 1
id: "REV-REQ-0031"
title: "A credential's scope is never widened beyond what it grants"
governed-by:
  - "REV-ADR-0028"
---

# A credential's scope is never widened beyond what it grants

## Statement

A credential that resolves to no scope, or whose scope claim grants nothing the account's role permits, MUST be refused
rather than granted the account's role-derived scope set. A credential's effective scope set MUST NOT be wider than what
the credential itself grants.

## Rationale

An authorisation system that quietly substitutes a broader scope set for a credential that failed to carry one, or that
narrowed to nothing, defeats the purpose of a scoped credential:
[RFC 6749 §3.3](https://www.rfc-editor.org/rfc/rfc6749#section-3.3) treats scope as the caller's delegated capability,
not a hint the server may override, and [RFC 9068 §2.2.3](https://www.rfc-editor.org/rfc/rfc9068#section-2.2.3) defines
the `scope` claim a JWT access token carries that delegation in. Falling back to the full role-derived set on a scope
failure would grant more capability than the credential's issuer delegated, violating least privilege. This obligation
is distinct from "Every API operation requires its declared scope" (REV-REQ-0005) and "API operations with a mutating
method require write scope" (REV-REQ-0006), which gate each operation on a declared scope requirement; this obligation
instead binds what a credential resolves to in the first place, before any per-operation gate runs.

## Acceptance criteria

- A credential carrying no scope at all is refused at every operation, never granted the operation's declared scope
  through some other check. Checked by `scopeless_token_rejected_by_every_op` in `backend/src/authz_matrix.rs`.
- A bearer access token whose scope claim is present but names no scope resolves to no scope set rather than the role's
  full one. Checked by `explicitly_empty_scope_list_rejects` in `backend/src/auth/middleware.rs`.
- A bearer access token whose scope claim narrows to nothing the account's role permits resolves to no scope set, not
  the role's full one. Checked by `claim_above_role_ceiling_rejects_rather_than_falling_back` in
  `backend/src/auth/middleware.rs`.
- A bearer access token that resolves to no scope set is refused with 401. The two tests above exercise the scope
  resolution alone; that its empty outcome is mapped to a rejected credential is verified by inspection of `resolve_jwt`
  in `backend/src/auth/middleware.rs`.
- A session always resolves to the account's full role-derived scope set; the session leg carries no separate scope
  claim to narrow. Verified by inspection of `CurrentUser::from_request_parts` in `backend/src/auth/middleware.rs`.

---
type: REQ
profile-version: 1
id: "REV-REQ-0015"
title: "A request that fails to decode answers the status its failure implies"
governed-by:
  - "REV-ADR-0011"
---

# A request that fails to decode answers the status its failure implies

## Statement

WHEN a request to an operation under `/api/v1` fails to decode before the operation runs, the response status MUST
reflect the kind of failure rather than a single flattened code: a malformed query parameter, a malformed path
parameter, or a request body that is not valid JSON each answer 400; a request body over the size limit answers 413; a
request body sent without an `application/json` content type answers 415; and a request body that is valid JSON but does
not match the operation's expected shape answers 422.

## Rationale

[RFC 9110](https://www.rfc-editor.org/rfc/rfc9110) §15.5 assigns each of these statuses a distinct recovery implication:
400 says the request's own grammar is at fault, 413 and 415 say the transport envelope is wrong before content is even
considered, and 422 (§15.5.21, scoped to request content) says the content parsed but does not fit the target shape.
Flattening every decode failure to one code would tell a caller building a query, a path, or a body that every failure
calls for the same fix, when in fact a 413 caller must shrink the payload, a 415 caller must change a header, and a 422
caller must change the payload's shape.

## Acceptance criteria

| Failure                                                               | Status |
| --------------------------------------------------------------------- | ------ |
| Malformed query parameter                                             | 400    |
| Malformed path parameter                                              | 400    |
| Request body that is not valid JSON                                   | 400    |
| Request body over the size limit                                      | 413    |
| Request body sent without an `application/json` content type          | 415    |
| Request body that is valid JSON but does not match the expected shape | 422    |

- A malformed path parameter answers 400. Checked by `malformed_path_returns_400_with_message_in_detail`
  (`backend/src/error/mod.rs`) and `malformed_path_parameter_is_400_malformed_path` (`backend/tests/extractors.rs`).
- A malformed query parameter answers 400. Checked by `malformed_query_returns_400_with_message_in_detail`
  (`backend/src/error/mod.rs`).
- A request body that is not valid JSON answers 400. Checked by `json_syntax_error_maps_to_400_invalid_request_body`
  (`backend/src/error/mod.rs`) and `invalid_json_syntax_is_400_invalid_request_body` (`backend/tests/extractors.rs`).
- A request body over the size limit answers 413. Checked by `oversized_json_body_maps_to_413_invalid_request_body`
  (`backend/src/error/mod.rs`) and `oversized_body_is_413_invalid_request_body` (`backend/tests/extractors.rs`).
- A request body sent without an `application/json` content type answers 415. Checked by
  `missing_json_content_type_maps_to_415_invalid_request_body` (`backend/src/error/mod.rs`) and
  `wrong_content_type_is_415_invalid_request_body` (`backend/tests/extractors.rs`).
- A request body that is valid JSON but does not match the expected shape answers 422. Checked by
  `json_data_error_maps_to_422_invalid_request_body` (`backend/src/error/mod.rs`) and
  `wrong_field_type_is_422_invalid_request_body` (`backend/tests/extractors.rs`).
- A well-formed request is not rejected by this decode boundary at all: it reaches the operation. Checked by
  `valid_request_reaches_the_handler` (`backend/tests/extractors.rs`).

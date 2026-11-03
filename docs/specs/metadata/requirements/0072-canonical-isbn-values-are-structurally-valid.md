---
type: REQ
profile-version: 1
id: "REV-REQ-0072"
title: "Canonical ISBN values are structurally valid"
---

# Canonical ISBN values are structurally valid

## Statement

When populating or changing canonical ISBN metadata, Reverie MUST admit a supplied value on structural-validity grounds
if and only if it conforms to the named field's ISBN format after ignoring ASCII spaces and hyphens. ISBN-10 requires
its prescribed length, characters and checksum; ISBN-13 requires thirteen decimal digits, a `978` or `979` prefix and a
valid ISBN-13 checksum. Structurally invalid values MUST NOT become canonical ISBN metadata.

## Rationale

Catalogue operations depend on distinguishing usable ISBN numbers from malformed input. Structural validation detects
malformed numbers but cannot establish whether a valid number was assigned to the publication being catalogued.

## Acceptance criteria

1. A structurally valid ISBN-10 or ISBN-13 passes validation for its corresponding field, including when the supplied
   number contains ASCII spaces or hyphens. Other eligibility rules still apply.
2. A value with the wrong length, disallowed characters or an invalid checksum fails validation for the targeted field
   and does not become canonical metadata.
3. A checksum-valid thirteen-digit value outside the `978` and `979` prefixes fails ISBN-13 validation and does not
   become canonical metadata.
4. These validity conditions apply during initial ingestion, automated enrichment, manual editing, acceptance and
   reversion.

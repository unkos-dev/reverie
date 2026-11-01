---
type: REQ
profile-version: 1
id: "REV-REQ-0071"
title: "The production build's CSP hash matches its pre-paint theme script"
governed-by:
  - "REV-ADR-0003"
---

# The production build's CSP hash matches its pre-paint theme script

## Statement

When producing a production frontend build, the frontend build pipeline MUST emit a CSP hash set containing a hash that
matches the exact text content of the pre-paint theme script in the emitted `index.html`. The pre-paint theme script is
the inline script that resolves the reader's theme and applies it before the application mounts.

## Rationale

A mismatched hash causes the browser to block the script under the production CSP. The application can subsequently
restore the theme, but the reader sees the initial page in the wrong theme. Keeping the script and its authorisation
consistent preserves pre-paint theme resolution without relaxing the CSP.

## Acceptance criteria

- For the completed production build, computing SHA-256 over the UTF-8 text content of the emitted pre-paint theme
  script and encoding the digest as standard base64 produces a `sha256-…` entry present in `csp-hashes.json`'s
  `script-src-hashes` array.
- Verification examines output produced by the actual production frontend configuration.

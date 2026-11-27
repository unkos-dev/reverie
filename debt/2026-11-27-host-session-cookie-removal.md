---
severity: low
surfaces: [developer]
adopted: 2026-11-27
adopted-because: tower-sessions 0.15.0 omits Secure from session removal cookies
lift-when-class: dep-unblocks
lift-when: Reverie adopts a tower-sessions release containing upstream pull request 268
---

# Host session cookie removal

tower-sessions 0.15.0 removes its session cookie without Secure. Browsers reject that removal for `__Host-id`, leaving
the expired session cookie in the browser even after the server deletes the session.

Reverie adds Secure to the final `__Host-id` removal header after the session layer runs. The correction leaves Path,
expiry and unrelated cookie headers intact. Direct LAN HTTP retains the library's `id` removal.

[The upstream fix](https://github.com/maxcountryman/tower-sessions/pull/268) supplies Secure during removal. When
Reverie adopts a release containing it, remove the response correction and its registration. Verify the final removal
header once against the dev server, then remove this entry and its README link in the same change.

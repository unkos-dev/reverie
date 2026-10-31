---
severity: medium
surfaces: [end-user]
adopted: 2026-10-27
adopted-because: a confirmation opened from a React action becomes inert before its modal effect takes ownership
lift-when-class: dep-unblocks
lift-when: the installed React and React Aria versions pass the metadata confirmation browser cases with the local synchronous mount removed and ViewTransition enabled
---

# Metadata confirmation modal ordering

`EditMetadataDialog` synchronously mounts its clear confirmation with `flushSync`. The call covers only opening that
confirmation. Requests remain awaited actions, and React Aria owns focus, dismissal and background isolation.

With React 19.3.1 and React Aria 3.53.1, the parent modal's mutation observer marks the new confirmation overlay inert
before the child modal's effect runs. The child then hides the parent, leaving both overlays inert. `ViewTransition`
defers effects, exposing this ordering when the confirmation opens from a form action. The native browser regression
includes the application's transition boundary and reproduces the failure without this call.

React documents
[`flushSync`](https://react.dev/reference/react-dom/flushSync#flushing-updates-for-third-party-integrations) as a
last-resort integration mechanism. It can flush other pending work, so it stays local to this confirmed failure. No code
removes `inert`, changes the portal container or disables router transitions.

Remove the call when the installed dependencies pass all cases in `EditMetadataDialog.test.tsx` without it. Verify
cancellation, pending suppression, refusal feedback, retained values and successful retry with the transition boundary
enabled, then delete this entry and its README link.

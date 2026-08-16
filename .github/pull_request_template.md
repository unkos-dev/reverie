<!-- markdownlint-disable-file MD041 -- PR template is a fragment, not a standalone document; top-level h1 is not wanted here. -->

## Summary

<!-- 1-3 bullet points describing what this PR does. Keep it tight. -->

-

## Why

<!-- Optional: keep only when motivation or context is not obvious from the diff. Delete the heading and this comment when irrelevant. -->

## Test plan

<!-- Bulleted checklist of how this was tested or how a reviewer can verify it. -->

- [ ]

## Accessibility

<!-- Required for UI-touching PRs. Delete this whole section for backend-only,
docs-only, or other non-UI changes. Process:
adr/superseded/2026-06-05-accessibility-review-process.md (gate mechanism
superseded by adr/2026-07-13-a11y-gate-on-playwright.md). -->

- [ ] Keyboard navigation reaches every interactive element.
- [ ] Focus is visible on every interactive element (gold 3 px ring per DESIGN.md).
- [ ] Non-text UI controls meet 1.4.11 (3:1) against the adjacent surface.
- [ ] Body text meets 1.4.3 (4.5:1) — except the documented gold-on-Parchment carve-out (large CTAs only).
- [ ] Motion respects `prefers-reduced-motion`.
- [ ] Any new colour is already a design token (no arbitrary hex).
- [ ] Reverie Alarm appears only in one of its two carve-out contexts.

<!-- Optional tracking issue: add `Closes UNK-NNN` when this work is tracked.
Delete this comment when it is not. Never submit placeholder or N/A content. -->

<!--
Reminders (see CONTRIBUTING.md for the full conventions):

- Title MUST follow Conventional Commits: <type>(<scope>): <description>
  Types: feat, fix, refactor, docs, chore, test, perf
- Include the tracking issue ID in the branch name when applicable
  (e.g. feat/unk-42-epub-import) and reference it in the body where relevant.
- Tests are mandatory for backend/ and frontend/: happy path, negative
  cases, and non-obvious edge cases. Tooling elsewhere is judged on
  whether it can fail quietly (AGENTS.md hard rule 5).
- Maintainer review and approval gate every merge.
-->

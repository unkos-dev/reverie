<!-- rumdl-disable-file MD041 -->
<!-- PR template is a fragment, not a standalone document; top-level h1 is not wanted here. -->

<!--
The title and description become the squash commit message. Write the
description as a durable account of why the change was needed and what resulted.
Keep review discussion in comments and delete optional sections that do not
apply.
-->

## Summary

<!-- 1-3 bullet points describing what this PR does. Keep it tight. -->

-

## Why

<!-- Optional: keep only when motivation or context is not obvious from the diff. Delete the heading and this comment when irrelevant. -->

## Validation

<!--
Record checks actually run and their results. State material verification gaps
and their effect on readiness. Keep pending CI status and downstream follow-up
out of this section.
-->

-

## Accessibility

<!-- Required for UI-touching PRs. Delete this whole section for backend-only,
docs-only, or other non-UI changes. -->

- [ ] Keyboard navigation reaches every interactive element.
- [ ] Focus is visible on every interactive element (gold 3 px ring per DESIGN.md).
- [ ] Non-text UI controls meet 1.4.11 (3:1) against the adjacent surface.
- [ ] Body text meets 1.4.3 (4.5:1).
- [ ] Motion respects `prefers-reduced-motion`.
- [ ] Any new colour is already a design token (no arbitrary hex).
- [ ] The danger hue appears only in a DESIGN.md carve-out context.

<!--
Reminders (see CONTRIBUTING.md for the full conventions):

- Title MUST follow Conventional Commits: <type>(<scope>): <description>
  Types: feat, fix, refactor, docs, chore, test, perf
- Product tests in backend/ and frontend/ need an anchor: an incident,
  a Requirement acceptance criterion, or a listed boundary. Tooling
  elsewhere is judged on whether it can fail quietly (AGENTS.md hard
  rule 5).
- Maintainer review and approval gate every merge.
-->

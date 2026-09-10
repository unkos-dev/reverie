---
type: ADR
profile-version: 1
id: "REV-ADR-0048"
title: "Use rumdl for Markdown linting and formatting"
status: "proposed"
recorded-on: "2026-09-10"
decision-makers:
  - "John Unkovich"
---

# Use rumdl for Markdown linting and formatting

## Context and problem statement

Markdown needs one formatter whose output the local and CI gates both accept. ADR 0032 assigns Markdown to oxfmt with
preserved wrapping. The preferred Markdown policy instead normalises prose to 120 columns and compacts wide tables
without rejecting their source-line length. markdownlint-cli2 also brings an exactly pinned vulnerable TOML parser,
while rumdl provides Markdown linting and formatting without that npm dependency.

## Decision drivers

- Use rumdl for Markdown linting and formatting.
- Enforce 120-column normalised prose reflow consistently in hooks, local checks and CI.
- Preserve table content, generated documents and upstream-owned text.
- Keep one formatter responsible for each file type.

## Considered options

- rumdl owns Markdown linting and formatting.
- rumdl lints Markdown while oxfmt preserves its formatting.
- Retain markdownlint-cli2 and override its vulnerable transitive dependency.

## Decision outcome

Chosen option: **rumdl owns Markdown linting and formatting**, because it implements the preferred reflow policy and
removes markdownlint-cli2 without a transitive dependency override. Using rumdl only as a linter would retain the
preserved-wrapping policy instead of normalised reflow.

This decision replaces the Markdown ownership and preserved-wrapping portions of
[Adopt oxfmt, replacing Prettier](0032-adopt-oxfmt-replacing-prettier.md). oxfmt retains the other supported file types,
including MDX; Rust remains on cargo fmt.

Prose uses normalised reflow at 120 columns. Tables retain structural checks and use aligned formatting with automatic
compaction above 120 columns. Separator widths follow the headers. Table rows and code blocks are exempt from
line-length checks. Generated Specful indexes and existing upstream-content exclusions remain outside the Markdown
formatter.

### Consequences

- Positive: Markdown formatting and linting share one tool and the local and CI policy agree.
- Positive: Removing markdownlint-cli2 also removes its vulnerable smol-toml dependency.
- Negative: Adopting normalised reflow creates a large formatting diff in the existing corpus.
- Negative: Contributors need the mise-pinned rumdl binary as well as the JavaScript tools.
- Negative: rumdl currently needs explicit automatic header alignment to compact already aligned tables; the workaround
  and its removal condition are tracked in debt.

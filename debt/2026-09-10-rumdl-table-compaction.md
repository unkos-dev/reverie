---
severity: low
surfaces: [developer, ci]
adopted: 2026-09-10
adopted-because: rumdl skips width checks for already aligned tables
lift-when-class: dep-unblocks
lift-when: the pinned rumdl compacts an already aligned table wider than 120 columns without explicit header alignment
---

# Explicit header alignment enables table compaction

The rumdl configuration sets `column-align-header = "auto"` to preserve automatic alignment while forcing the table
formatter to evaluate `max-width`. In rumdl 0.2.70, already aligned tables otherwise return before the width check.

Remove this setting and entry when the pinned release compacts an already aligned table wider than 120 columns without
the setting. Verify with `rumdl check --enable MD060 --diff` and the repository Markdown gate.

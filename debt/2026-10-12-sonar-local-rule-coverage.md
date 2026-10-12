---
severity: low
surfaces: [developer, ci]
adopted: 2026-10-12
adopted-because: Oxlint JavaScript plugins lack TypeScript parser services, and SonarJS assertion detection does not recognise vite-plus/test imports.
lift-when-class: dep-unblocks
lift-when: The Vite+ lint gate detects mutable React props, optional undefined through local type aliases, and non-length specific-assertion cases imported from vite-plus/test, while respecting exactOptionalPropertyTypes.
---

# Sonar rules with partial local coverage

The root Vite+ configuration runs SonarJS's nested-conditional, nested-template and redundant-optional rules through
Oxlint's JavaScript plugin interface. The native Vitest length-assertion rule covers length comparisons imported from
`vite-plus/test`. These rules run in the existing local and CI frontend lint gate.

Three Sonar rules retain gaps in local enforcement:

- **S6759:** `prefer-read-only-props` returns no checks without TypeScript parser services. It remains enabled in Sonar
  and is not enabled locally. Oxlint's native type-aware mode does not provide those services to JavaScript plugins.
- **S4782:** `no-redundant-optional` detects explicit optional unions containing `undefined` locally. It cannot resolve
  aliases or inspect `exactOptionalPropertyTypes` without parser services. The current frontend configuration leaves
  that option disabled; enabling it requires revisiting this local rule.
- **S5906:** native `vitest/prefer-to-have-length` covers length assertions. SonarJS's broader
  `prefer-specific-assertions` does not recognise `vite-plus/test` imports and remains disabled locally. Other assertion
  shapes still rely on Sonar.

These are exceptions to local and remote lint parity. Remove this entry when the existing Vite+ gate can enforce all
three rules, verified against both violating and compliant examples, without adding another lint runner.

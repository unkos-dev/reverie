---
name: Reverie
description: A household ebook collection, ready to curate.
colors:
  shell: "var(--shell-ground)"
  sheet: "var(--sheet-ground)"
  surface: "var(--surface)"
  surface-2: "var(--surface-2)"
  surface-3: "var(--surface-3)"
  hairline: "var(--hairline)"
  control-border: "var(--border-control)"
  foreground: "var(--fg)"
  muted: "var(--fg-muted)"
  faint: "var(--fg-faint)"
  accent: "var(--accent)"
  accent-soft: "var(--accent-soft)"
  accent-strong: "var(--accent-strong)"
  accent-text: "var(--accent-text)"
  on-accent: "var(--fg-on-accent)"
  danger: "var(--danger)"
  danger-soft: "var(--danger-soft)"
  danger-text: "var(--danger-text)"
  bar: "var(--bar)"
  bar-raised: "var(--bar-raised)"
  overlay: "var(--overlay)"
  neutral-dark: "#eef1fa"
  accent-dark: "#8fa8ff"
  danger-dark: "#ff8a93"
  neutral-light: "#121831"
  accent-light: "#3d55d8"
  danger-light: "#b91c1c"
  neutral-monochrome: "#f3f3f5"
  accent-monochrome: "#f3f3f5"
  danger-monochrome: "#ff8a93"
typography:
  display:
    fontFamily: '"Mona Sans", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "2.25rem"
    fontWeight: 800
    lineHeight: 1.111111
    letterSpacing: "-0.025em"
  title:
    fontFamily: '"Mona Sans", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "1rem"
    fontWeight: 600
    lineHeight: 1.5
  body:
    fontFamily: '"Mona Sans", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "1rem"
    fontWeight: 400
    lineHeight: 1.5
  table-cell:
    fontFamily: '"Mona Sans", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: 1.428571
  label:
    fontFamily: '"Mona Sans", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "0.75rem"
    fontWeight: 500
    lineHeight: 1.333333
  metadata:
    fontFamily: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace'
    fontSize: "0.75rem"
    fontWeight: 400
    lineHeight: 1.333333
  wordmark:
    fontFamily: '"Satoshi Variable", "Satoshi", system-ui, sans-serif'
    fontSize: "1rem"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "0.32em"
  cover-title:
    fontFamily: '"Author Variable", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontWeight: 600
    lineHeight: 1.06
    letterSpacing: "0.005em"
rounded:
  sm: "9px"
  md: "12px"
  lg: "16px"
  xl: "18px"
spacing:
  '1': "4px"
  '2': "8px"
  '3': "12px"
  '4': "16px"
  '5': "20px"
  '6': "24px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.on-accent}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "34px"
  button-outline:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.foreground}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "34px"
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.foreground}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "34px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.muted}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "34px"
  button-destructive:
    backgroundColor: "{colors.danger-soft}"
    textColor: "{colors.danger-text}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "34px"
  button-link:
    backgroundColor: "transparent"
    textColor: "{colors.accent-text}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "34px"
  input:
    textColor: "{colors.foreground}"
    rounded: "{rounded.sm}"
    padding: "4px 10px"
    height: "34px"
  card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.foreground}"
    rounded: "{rounded.xl}"
    padding: "16px"
  navigation-current:
    textColor: "{colors.foreground}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "36px"
  filter-chip:
    backgroundColor: "transparent"
    textColor: "{colors.muted}"
    rounded: "{rounded.lg}"
    padding: "4px 10px"
---

# Design System: Reverie

<!-- impeccable:design-map -->
Name: Reverie Colors: primary: accent var(--accent); neutral: shell var(--shell-ground); error: danger var(--danger);
primary: accent-soft var(--accent-soft); primary: accent-strong var(--accent-strong) (+25 more in the sidecar) Fonts:
Mona Sans (display, title, body, table-cell, label); JetBrains Mono (metadata); Satoshi Variable (wordmark); Author
Variable (cover-title) Spacing: 1 4px, 2 8px, 3 12px, 4 16px, 5 20px, 6 24px Radius: sm 9px, md 12px, lg 16px, xl 18px
North Star: "The book collection". Rules:

- The semantic colour rule. Interface elements use semantic roles; fixed artwork keeps its own palette.
- The floating depth rule. Resting cards stay flat; floating controls carry the raised shadow. Don't:
- Don't apply the generated covers' decorative texture or metallic detail to interface controls.
- Don't use the error hue as a general attention accent. Do:
- Do keep interface colours on semantic roles so all palettes use the same components.
- Do keep book artwork and its generated-cover texture separate from interface styling.
- Do use the raised shadow for floating controls and flat borders for resting cards.
- Do keep edit feedback in its owning context and preserve reduced-motion alternatives. Full: .impeccable/design-full.md
  (read a section when you need its detail)

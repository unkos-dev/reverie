---
type: ADR
profile-version: 1
id: "REV-ADR-0072"
title: "Compile the frontend with the React Compiler through oxc"
status: "accepted"
recorded-on: "2026-11-10"
decided-on: "2026-11-10"
decision-makers:
  - "John Unkovich"
---

# Compile the frontend with the React Compiler through oxc

## Context and problem statement

The frontend caches a value or a component between renders only where a developer writes `memo`, `useMemo` or
`useCallback` by hand. The Library table shows the cost. Its cells read one context, and the table built that context
value afresh on every render, so every visible cell rendered again whenever the table did. React Aria's `Table` skips
rendering a row whose inputs are unchanged, and the new context value counted as a change every time. Static analysis
flagged the context value, and the same pattern can appear in any component.

[Adopt oxlint, replacing the ESLint toolchain](./0030-adopt-oxlint-replacing-the-eslint-toolchain.md) already enforces
the React Compiler's rules as lint errors, so the code follows the rules a compiler needs. It left enabling the compiler
transform as a follow-up.

Should the React Compiler add this caching to the frontend, and if so, through which implementation?

## Decision drivers

- Caching follows the code's data flow without a developer maintaining dependency lists.
- The app build, the dev server and every test project compile components the same way.
- The frontend keeps to the oxc and VoidZero toolchain it already runs on, as recorded in
  [Adopt the Vite+ (vp) monorepo toolchain](./0033-adopt-the-vite-vp-monorepo-toolchain.md).
- Each extra transform pass or dependency earns its place.

## Considered options

- **oxc's Rust React Compiler**: `@vitejs/plugin-react` with `compiler: true`, which runs `oxc-transform-react`.
- **Babel React Compiler**: the React team's `babel-plugin-react-compiler` 1.0.0, applied through the plugin's
  `reactCompilerPreset` with `@rolldown/plugin-babel` and `@babel/core`.
- **No compiler**: add `memo`, `useMemo` or `useCallback` by hand where profiling shows a need.

## Decision outcome

Chosen option: **oxc's Rust React Compiler**, because it comes from the oxc and VoidZero toolchain that already supplies
Vite+, oxlint and oxfmt, and it plugs into the React plugin the frontend already calls. It adds one optional peer
dependency and no Babel pass. The compiler caches the Library table's context value, its callbacks, and the hook results
they depend on.

Existing hand-written `memo`, `useMemo` and `useCallback` stay in place, as the React Compiler adoption guidance
advises.

### Consequences

- Positive: the compiler caches values, callbacks and JSX in components and hooks by default, including values that
  hand-written `useMemo` and `useCallback` missed.
- Positive: the lint rules and the transform now come from the same compiler model, so code that passes lint compiles.
- Negative: `@vitejs/plugin-react` labels its native compiler support experimental.
- Negative: the shipped and served JavaScript differs from the source. Debugging relies on source maps, and a stack
  trace can point into compiler-generated cache code.
- Negative: the production JavaScript grows. On the commit that adopted the compiler, the build's JavaScript went from
  1,450.87 kB to 1,603.46 kB raw (+10.5%) and from 457.78 kB to 520.23 kB gzipped (+13.6%). The entry chunk grew by 3.27
  kB gzipped, and the Library route's two largest chunks grew by 21.13 kB gzipped between them. Build time did not
  change measurably.
- Negative: `@vitejs/plugin-react` accepts only a narrow range of `oxc-transform-react` versions, which trails oxc's
  weekly releases. The compiler can update only within that range until the plugin widens it.
- Negative: a component the compiler cannot optimise compiles without caching and without a warning, because the plugin
  reports recoverable diagnostics only when `logDiagnostics` is set. The root `App` component is one, because it passes
  the `useHref` hook from React Router to the React Aria `RouterProvider` as a value.

## Pros and cons of the options

### oxc's Rust React Compiler

- Positive: it is a setting on the existing React plugin, and it runs in the same oxc transform as JSX and Fast Refresh.
- Negative: it is experimental, and its version is tied to the range the plugin accepts.

### Babel React Compiler

- Positive: it is the stable 1.0.0 release, and react.dev documents it as the route for Vite.
- Negative: it adds three dependencies and runs a Babel pass over every transformed module.
- Neutral: switching to it is a configuration change in the same plugin, with no change to application code.

### No compiler

- Positive: the shipped code matches the source, and the bundle stays its current size.
- Negative: each missed `useMemo` or `useCallback`, like the Library table's context value, waits for profiling or
  review to find it.

## More information

- [`@vitejs/plugin-react` React Compiler options](https://github.com/vitejs/vite-plugin-react/tree/main/packages/plugin-react#react-compiler):
  the `compiler` option, `logDiagnostics`, and the Babel preset.
- [React Compiler introduction](https://react.dev/learn/react-compiler/introduction): adoption guidance on existing
  hand-written `useMemo` and `useCallback`.

Revisit trigger: if the native compiler produces a behaviour defect, or the plugin stops tracking `oxc-transform-react`
releases, reopen the choice between the two compilers with a new ADR.

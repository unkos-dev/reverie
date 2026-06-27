/**
 * Dev-only react-query devtools panel.
 *
 * Lazy-imported in `main.tsx` behind `import.meta.env.DEV`; in
 * production the entire module is dead code (the `lazy()` call site
 * is unreachable, Vite tree-shakes the import). Living in its own
 * module keeps `main.tsx` exporting nothing but the side-effecting
 * `createRoot()` call, which avoids the
 * `react/only-export-components` rule.
 */
import { lazy, Suspense, type ComponentType, type ReactElement } from "react";

const Devtools: ComponentType = lazy(() =>
  import("@tanstack/react-query-devtools").then((mod) => ({
    default: mod.ReactQueryDevtools,
  })),
);

/** Renders the devtools panel inside a `<Suspense>` boundary. */
export function ReactQueryDevtoolsPanel(): ReactElement {
  return (
    <Suspense fallback={null}>
      <Devtools />
    </Suspense>
  );
}

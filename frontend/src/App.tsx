import type { ReactElement } from "react";

/**
 * Root route component (`/`).
 *
 * Renders the application shell that the library view will mount into.
 * Currently a brand-canvas placeholder — Step 11 (UNK-80) wires the
 * library list/detail UI into this surface. Kept deliberately empty
 * so the canvas + foreground tokens, the FOUC theme bootstrap, and the
 * router/provider tree can be sighted against a stable baseline
 * between hero-screen iterations and the Step 11 build-out.
 */
function App(): ReactElement {
  return (
    <main className="bg-canvas text-fg min-h-screen">
      {/* Step 11 builds the library view here. */}
    </main>
  );
}

export default App;

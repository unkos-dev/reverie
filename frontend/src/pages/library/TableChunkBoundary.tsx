/**
 * Error boundary scoped to the lazy-loaded table view.
 *
 * The table view is the library's only code-split view; its most likely
 * runtime failure is the chunk itself failing to load (a deploy rotating
 * asset hashes under a cached client, a network blip). Without a local
 * boundary that rejection would unmount the whole library page onto the
 * route-level error screen; this keeps the failure inside the table slot
 * with the rest of the page (filters, view toggle) usable.
 *
 * A class component is the deliberate exception to the functional-only
 * rule: React exposes error boundaries only through the class lifecycle,
 * and the Suspense/ErrorBoundary pairing rule takes precedence.
 */
import { Component, type ReactElement, type ReactNode } from "react";

import { Button } from "@/components/ui/button";

type Props = {
  children: ReactNode;
  /** Escape hatch back to a view whose code is already loaded. */
  onFallbackToGrid: () => void;
};

type State = { failed: boolean };

export class TableChunkBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(error: unknown): void {
    // QueryCache.onError only forwards 401s; keep the developer breadcrumb
    // convention for everything that degrades to inline UI.
    console.error("[TableChunkBoundary] table view failed to load", error);
  }

  render(): ReactElement | ReactNode {
    if (!this.state.failed) return this.props.children;
    return (
      <div
        role="alert"
        className="border-border text-fg-muted flex min-h-48 flex-col items-center justify-center gap-4 rounded-md border border-dashed py-10 text-center"
      >
        <p className="text-sm">Couldn&apos;t load the table view.</p>
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => {
              this.setState({ failed: false });
            }}
          >
            Try again
          </Button>
          <Button type="button" variant="ghost" size="sm" onClick={this.props.onFallbackToGrid}>
            Switch to grid view
          </Button>
        </div>
      </div>
    );
  }
}

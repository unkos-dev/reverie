import type { ReactElement, ReactNode } from "react";
import { ErrorBoundary } from "react-error-boundary";

import { Button } from "@/components/ui/button";

type Props = {
  children: ReactNode;
  /** Escape hatch back to a view whose code is already loaded. */
  onFallbackToGrid: () => void;
};

function TableFailure({
  onFallbackToGrid,
}: Readonly<Pick<Props, "onFallbackToGrid">>): ReactElement {
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
          onPress={() => {
            window.location.reload();
          }}
        >
          Reload page
        </Button>
        <Button type="button" variant="ghost" size="sm" onPress={onFallbackToGrid}>
          Switch to grid view
        </Button>
      </div>
    </div>
  );
}

/** Keeps table failures local so Library controls remain available. */
export function TableChunkBoundary({ children, onFallbackToGrid }: Readonly<Props>): ReactElement {
  return (
    <ErrorBoundary
      onError={(error: unknown) => {
        console.error("[TableChunkBoundary] table view failed to load", error);
      }}
      fallback={<TableFailure onFallbackToGrid={onFallbackToGrid} />}
    >
      {children}
    </ErrorBoundary>
  );
}

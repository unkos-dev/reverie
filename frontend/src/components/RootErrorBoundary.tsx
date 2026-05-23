/**
 * Root-level error surface for the data-mode router.
 *
 * react-router catches `throw new Response(...)` from loaders and
 * unhandled exceptions from `Component` renders and presents them
 * via the nearest `errorElement` (or `ErrorBoundary` on a route
 * record). Without one, the user sees an unstyled router default;
 * this component is the branded fallback mounted at the root route.
 *
 * Three branches:
 * 1. Route response (`isRouteErrorResponse`) — surfaces the HTTP
 *    status the loader threw, with a 404-specific copy variant.
 * 2. `ApiError` — propagated from a `useSuspenseQuery` failure;
 *    surfaces the parsed RFC 7807 `title` / `detail`.
 * 3. Anything else — a generic "something went wrong" surface; the
 *    raw error is `console.error`-logged for debugging.
 */
import type { ReactElement } from "react";
import { Link, isRouteErrorResponse, useRouteError } from "react-router";

import { ApiError } from "@/api";
import { Button } from "@/components/ui/button";

/**
 * Root `errorElement` for the data-mode router. Distinguishes
 * route-response errors (`isRouteErrorResponse`) from API errors
 * (`ApiError`) from generic exceptions, and surfaces a branded
 * fallback for each.
 */
export function RootErrorBoundary(): ReactElement {
  const error = useRouteError();

  if (isRouteErrorResponse(error)) {
    const is404 = error.status === 404;
    return (
      <ErrorSurface
        title={is404 ? "Not found" : `Error ${String(error.status)}`}
        detail={
          is404
            ? "We couldn't find what you were looking for. It may have been removed or you may not have permission to see it."
            : error.statusText || "An unexpected error occurred."
        }
      />
    );
  }

  if (error instanceof ApiError) {
    return (
      <ErrorSurface
        title={error.title}
        detail={error.detail !== "" ? error.detail : "An unexpected error occurred."}
      />
    );
  }

  // Surface a clean message; log the raw cause so it's recoverable
  // from devtools without polluting the UI.
  console.error("RootErrorBoundary unhandled error:", error);
  return (
    <ErrorSurface
      title="Something went wrong"
      detail="An unexpected error occurred. Try reloading the page."
    />
  );
}

interface ErrorSurfaceProps {
  title: string;
  detail: string;
}

function ErrorSurface({ title, detail }: ErrorSurfaceProps): ReactElement {
  return (
    <div className="bg-canvas text-fg flex min-h-screen items-center justify-center px-6 py-16">
      <div className="border-border bg-surface-1 max-w-lg rounded-md border p-8 text-center">
        <h1 className="font-display text-fg mb-3 text-2xl font-semibold tracking-tight">{title}</h1>
        <p className="text-fg-muted mb-6 text-sm leading-relaxed">{detail}</p>
        <div className="flex justify-center gap-3">
          <Button asChild variant="outline">
            <Link to="/library">Back to library</Link>
          </Button>
        </div>
      </div>
    </div>
  );
}

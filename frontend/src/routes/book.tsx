/**
 * Route module for `/b/:id`. Exports `loader` (prefetches the book
 * detail into the shared `QueryClient`) and `Component` (the page
 * renderer).
 *
 * react-router data-mode requires the loader and component to live
 * in the same module so a single dynamic `import()` resolves both;
 * `react-refresh/only-export-components` is disabled here because the
 * mixed export shape is required by the router contract.
 */
/* eslint-disable react-refresh/only-export-components */
import type { LoaderFunctionArgs } from "react-router";

import { getBook } from "@/api";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import { BookPage } from "@/pages/book/BookPage";

/**
 * Loader for `/b/:id`. Prefetches the detail so the component's
 * `useSuspenseQuery` hits a hot cache. Returns `null`; the component
 * reads the id from `useParams`.
 *
 * Throws a `Response` on a missing id — react-router's documented
 * mechanism for bailing out of a loader straight into the nearest
 * `errorElement` boundary. `@typescript-eslint/only-throw-error` is
 * disabled for that single line because the framework-prescribed
 * value is intentionally not an `Error` subclass.
 */
export async function loader({ params }: LoaderFunctionArgs): Promise<null> {
  const id = params.id;
  if (typeof id !== "string" || id.length === 0) {
    // eslint-disable-next-line @typescript-eslint/only-throw-error -- react-router's loader-bailout convention is `throw new Response(...)`.
    throw new Response("Missing book id", { status: 404 });
  }
  await queryClient.prefetchQuery({
    queryKey: queryKeys.books.detail(id),
    queryFn: ({ signal }) => getBook(id, signal),
  });
  return null;
}

export { BookPage as Component };

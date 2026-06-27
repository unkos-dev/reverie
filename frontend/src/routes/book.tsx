/**
 * Route module for `/b/:id`. Exports `loader` (prefetches the book
 * detail into the shared `QueryClient`) and `Component` (the page
 * renderer).
 *
 * react-router data-mode requires the loader and component to live
 * in the same module so a single dynamic `import()` resolves both;
 * `react/only-export-components` is disabled here because the
 * mixed export shape is required by the router contract.
 */
/* oxlint-disable react/only-export-components */
import type { LoaderFunctionArgs } from "react-router";

import { getBook, type BookDetail } from "@/api";
import type { TitleData } from "@/components/shell/crumbs";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import { BookPage } from "@/pages/book/BookPage";

/**
 * Loader for `/b/:id`. Prefetches the detail so the component's
 * `useSuspenseQuery` hits a hot cache, then returns `{ title }` for
 * the utility strip's breadcrumb (the component itself reads the id
 * from `useParams`). `prefetchQuery` swallows fetch errors, so the
 * post-prefetch cache read can be `undefined` — that degrades to
 * `null` and the breadcrumb renders without a trailing label.
 *
 * Throws a `Response` on a missing id — react-router's documented
 * mechanism for bailing out of a loader straight into the nearest
 * `errorElement` boundary. `typescript/only-throw-error` is
 * disabled for that single line because the framework-prescribed
 * value is intentionally not an `Error` subclass.
 */
export async function loader({ params }: LoaderFunctionArgs): Promise<TitleData | null> {
  const id = params.id;
  if (typeof id !== "string" || id.length === 0) {
    // oxlint-disable-next-line typescript/only-throw-error -- react-router's loader-bailout convention is `throw new Response(...)`.
    throw new Response("Missing book id", { status: 404 });
  }
  await queryClient.prefetchQuery({
    queryKey: queryKeys.books.detail(id),
    queryFn: ({ signal }) => getBook(id, signal),
  });
  const detail = queryClient.getQueryData<BookDetail>(queryKeys.books.detail(id));
  return detail === undefined ? null : ({ title: detail.title } satisfies TitleData);
}

export { BookPage as Component };

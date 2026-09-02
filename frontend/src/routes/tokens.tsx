/* oxlint-disable react/only-export-components */
/**
 * Route module for `/tokens`.
 */
import { listTokens } from "@/api/tokens";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import { TokensPage } from "@/pages/tokens/TokensPage";

/** Loader for `/tokens` — prefetches the caller's token list. */
export async function loader(): Promise<null> {
  await queryClient
    .query({
      queryKey: queryKeys.tokens.list(),
      queryFn: ({ signal }) => listTokens(signal),
    })
    .catch(() => {});
  return null;
}

/** Component export consumed by the route's `lazy()` callback. */
export const Component = TokensPage;

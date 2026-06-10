/* eslint-disable react-refresh/only-export-components */
/**
 * Route module for `/admin/users`.
 */
import { listUsers } from "@/api/users";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import type { AuthMe } from "@/hooks/useAuthMe";
import { UsersPage } from "@/pages/admin/UsersPage";

/**
 * Loader for `/admin/users` — prefetches the user list for known admins.
 *
 * The prefetch is gated on the cached `me` identity: if the cache already
 * shows a non-admin role (or is cold/absent), we skip the prefetch and let
 * the component's `enabled: me?.role === "admin"` guard handle fetching.
 * This avoids firing a guaranteed-to-fail GET /api/v1/users for non-admin
 * visitors who navigate directly to this URL.
 */
export async function loader(): Promise<null> {
  const me = queryClient.getQueryData<AuthMe | null>(queryKeys.auth.me());
  if (me?.role === "admin") {
    await queryClient.prefetchQuery({
      queryKey: queryKeys.users.list(),
      queryFn: ({ signal }) => listUsers(signal),
    });
  }
  return null;
}

/** Component export consumed by the route's `lazy()` callback. */
export const Component = UsersPage;

/* oxlint-disable react/only-export-components */
/**
 * Route module for `/admin/dashboard`.
 */
import { getDashboardStats } from "@/api/dashboard";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import type { AuthMe } from "@/hooks/useAuthMe";
import { DashboardPage } from "@/pages/admin/DashboardPage";

/**
 * Loader for `/admin/dashboard` — prefetches the stats for known admins.
 *
 * Gated on the cached `me` identity: if the cache shows a non-admin role
 * (or is cold), the prefetch is skipped and the component's
 * `enabled: me?.role === "admin"` guard handles fetching. This avoids
 * firing a guaranteed-to-fail GET for non-admin visitors who navigate
 * directly to this URL.
 */
export async function loader(): Promise<null> {
  const me = queryClient.getQueryData<AuthMe | null>(queryKeys.auth.me());
  if (me?.role === "admin") {
    await queryClient
      .query({
        queryKey: queryKeys.dashboard.stats(),
        queryFn: ({ signal }) => getDashboardStats(signal),
      })
      .catch(() => {});
  }
  return null;
}

/** Component export consumed by the route's `lazy()` callback. */
export const Component = DashboardPage;

/* oxlint-disable react/only-export-components */
/**
 * Route module for `/admin/settings/:area`.
 */
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import type { AuthMe } from "@/hooks/useAuthMe";
import { settingsQueryOptions } from "@/pages/admin/settings/queries";
import { SettingsPage } from "@/pages/admin/settings/SettingsPage";

/**
 * Loader for `/admin/settings/:area` — fetches a fresh settings snapshot for
 * known admins on every entry, so a draft is always based on current values.
 *
 * Gated on the cached `me` identity like the other admin loaders: a cold or
 * non-admin cache skips the request and the component's own guard decides.
 */
export async function loader(): Promise<null> {
  const me = queryClient.getQueryData<AuthMe | null>(queryKeys.auth.me());
  if (me?.role === "admin") {
    await queryClient.query({ ...settingsQueryOptions(), staleTime: 0 }).catch(() => {});
  }
  return null;
}

/** Component export consumed by the route's `lazy()` callback. */
export const Component = SettingsPage;

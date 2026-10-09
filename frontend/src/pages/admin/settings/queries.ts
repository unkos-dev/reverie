import { queryOptions, type UseQueryOptions } from "@tanstack/react-query";

import { getReloadStatus, getSettings, type SettingsSnapshot } from "@/api/settings";
import { queryKeys } from "@/lib/query/keys";

/**
 * The settings snapshot is only ever refetched on purpose: a background
 * refetch would advance the `If-Match` tag past the snapshot a draft was
 * edited against, and the next save would silently overwrite another admin's
 * change instead of failing with 412.
 */
type SnapshotKey = ReturnType<typeof queryKeys.settings.snapshot>;
type ReloadKey = ReturnType<typeof queryKeys.settings.reloadStatus>;

export function settingsQueryOptions(): UseQueryOptions<
  SettingsSnapshot,
  Error,
  SettingsSnapshot,
  SnapshotKey
> {
  return queryOptions({
    queryKey: queryKeys.settings.snapshot(),
    queryFn: ({ signal }) => getSettings(signal),
    staleTime: Infinity,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });
}

export function reloadStatusQueryOptions(): UseQueryOptions<
  string | null,
  Error,
  string | null,
  ReloadKey
> {
  return queryOptions({
    queryKey: queryKeys.settings.reloadStatus(),
    queryFn: ({ signal }) => getReloadStatus(signal),
    staleTime: 30_000,
  });
}

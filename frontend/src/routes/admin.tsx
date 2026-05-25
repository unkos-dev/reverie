/* eslint-disable react-refresh/only-export-components */
/**
 * Route module for `/admin/users`.
 */
import { listUsers } from "@/api/users";
import { queryClient } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";
import { UsersPage } from "@/pages/admin/UsersPage";

/** Loader for `/admin/users` — prefetches the user list. */
export async function loader(): Promise<null> {
  await queryClient.prefetchQuery({
    queryKey: queryKeys.users.list(),
    queryFn: ({ signal }) => listUsers(signal),
  });
  return null;
}

/** Component export consumed by the route's `lazy()` callback. */
export const Component = UsersPage;

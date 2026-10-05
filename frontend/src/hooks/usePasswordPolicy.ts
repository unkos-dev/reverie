import { useQuery } from "@tanstack/react-query";

import { fetchSetupStatus, type SetupStatus } from "@/api/auth";
import { queryKeys } from "@/lib/query/keys";

export function usePasswordPolicy(): {
  policy: SetupStatus | undefined;
  isPending: boolean;
  isError: boolean;
} {
  const { data, isPending, isError } = useQuery({
    queryKey: queryKeys.auth.setupStatus(),
    queryFn: ({ signal }) => fetchSetupStatus(signal),
    retry: false,
  });
  return { policy: data, isPending, isError };
}

import { useQuery } from "@tanstack/react-query";

import { fetchSetupStatus, type SetupStatus } from "@/api/auth";
import { newPasswordField } from "@/api/auth.schemas";
import { queryKeys } from "@/lib/query/keys";

export type PasswordPolicyState = {
  policy: SetupStatus | undefined;
  ready: boolean;
  isError: boolean;
  retry: () => void;
  retrying: boolean;
  validate: (password: string) => string | undefined;
};

export function usePasswordPolicy(): PasswordPolicyState {
  const { data, isError, isFetching, isFetched, refetch } = useQuery({
    queryKey: queryKeys.auth.setupStatus(),
    queryFn: ({ signal }) => fetchSetupStatus(signal),
    retry: false,
  });
  function validate(password: string): string | undefined {
    if (data === undefined) return "Could not load the password policy. Reload to try again.";
    const parsed = newPasswordField(data).safeParse(password);
    return parsed.success
      ? undefined
      : (parsed.error.issues[0]?.message ?? "Check the password length.");
  }
  return {
    policy: data,
    retry: () => {
      void refetch();
    },
    retrying: isFetching && isFetched,
    ready: data !== undefined,
    isError: data === undefined && isError,
    validate,
  };
}

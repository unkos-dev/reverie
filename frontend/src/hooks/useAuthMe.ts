/**
 * Hook for the authenticated user's identity from `/auth/me`.
 *
 * Fetches once (staleTime: Infinity) and caches under `["auth", "me"]`.
 *
 * Response handling:
 * - 200 OK — parsed and returned as `AuthMe`.
 * - 401 / 403 — unauthenticated/forbidden; returns `null` (normal state).
 * - Any other non-2xx — throws so React Query surfaces the error and the
 *   caller can distinguish an operational failure from "not logged in".
 */
import { useQuery } from "@tanstack/react-query";
import { z } from "zod";

import { queryKeys } from "@/lib/query/keys";

const ROLE_VALUES = ["admin", "adult", "child"] as const;

const AuthMeSchema = z.object({
  id: z.uuid(),
  display_name: z.string(),
  email: z.string().nullable(),
  role: z.enum(ROLE_VALUES),
  is_child: z.boolean(),
  theme_preference: z.string(),
  csrf_token: z.string().nullable().optional(),
});

type AuthMe = z.infer<typeof AuthMeSchema>;

function useAuthMe(): { data: AuthMe | undefined; isLoading: boolean } {
  const { data, isLoading } = useQuery({
    queryKey: queryKeys.auth.me(),
    queryFn: async ({ signal }) => {
      const resp = await fetch("/auth/me", {
        credentials: "same-origin",
        signal,
      });
      if (resp.status === 401 || resp.status === 403) {
        // Not authenticated — callers treat undefined data as "logged out".
        return null;
      }
      if (!resp.ok) {
        throw new Error(`/auth/me failed: ${String(resp.status)} ${resp.statusText}`);
      }
      const raw: unknown = await resp.json();
      return AuthMeSchema.parse(raw);
    },
    staleTime: Infinity,
    retry: false,
  });
  return { data: data ?? undefined, isLoading };
}

export { useAuthMe };
export type { AuthMe };

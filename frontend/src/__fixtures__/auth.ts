import type { AuthMe } from "@/hooks/useAuthMe";

/**
 * Canonical authenticated-user stub for tests that mock `/auth/me`.
 *
 * `satisfies AuthMe` keeps this in lockstep with the schema: a field added to
 * or renamed in `AuthMeSchema` breaks compilation here rather than letting the
 * stub silently drift from the contract it stands in for.
 */
const STUB_ME = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  display_name: "Alice",
  email: "alice@example.com",
  role: "admin" as const,
  is_child: false,
  theme_preference: "system",
  csrf_token: null,
} satisfies AuthMe;

export { STUB_ME };

/**
 * Admin user-management API client (`/api/users*`).
 *
 * All endpoints require `role = admin`; non-admin callers receive
 * 403 Forbidden.
 */
import { z } from "zod";

import { apiFetch } from "./fetch";

const ROLE_VALUES = ["admin", "adult", "child"] as const;
type Role = (typeof ROLE_VALUES)[number];

const UserSchema = z.object({
  id: z.uuid(),
  display_name: z.string(),
  email: z.string().email().nullable(),
  role: z.enum(ROLE_VALUES),
  is_child: z.boolean(),
  created_at: z.string(),
  updated_at: z.string(),
});

type User = z.infer<typeof UserSchema>;

const UsersListSchema = z.array(UserSchema);

/** `GET /api/users` — list all users (admin only). */
async function listUsers(signal?: AbortSignal): Promise<User[]> {
  const raw = await apiFetch("/api/users", { signal });
  return UsersListSchema.parse(raw);
}

/** `PUT /api/users/{id}/role` — change a user's role (admin only). */
async function updateUserRole(id: string, role: Role, signal?: AbortSignal): Promise<User> {
  const raw = await apiFetch(`/api/users/${id}/role`, {
    method: "PUT",
    body: JSON.stringify({ role }),
    signal,
  });
  return UserSchema.parse(raw);
}

/** `PUT /api/users/{id}/child-status` — toggle child status (admin only). */
async function updateUserChildStatus(
  id: string,
  isChild: boolean,
  signal?: AbortSignal,
): Promise<User> {
  const raw = await apiFetch(`/api/users/${id}/child-status`, {
    method: "PUT",
    body: JSON.stringify({ is_child: isChild }),
    signal,
  });
  return UserSchema.parse(raw);
}

/** Fields accepted by `PATCH /api/users/{id}`. */
interface UpdateUserFields {
  display_name?: string | null;
  email?: string | null;
}

/** `PATCH /api/users/{id}` — update display name / email (admin only). */
async function updateUser(
  id: string,
  fields: UpdateUserFields,
  signal?: AbortSignal,
): Promise<User> {
  const raw = await apiFetch(`/api/users/${id}`, {
    method: "PATCH",
    body: JSON.stringify(fields),
    signal,
  });
  return UserSchema.parse(raw);
}

export { listUsers, updateUserRole, updateUserChildStatus, updateUser };
export type { User, Role, UpdateUserFields };

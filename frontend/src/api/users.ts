/**
 * Admin user-management API client (`/api/v1/users*`).
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
  email: z.email().nullable(),
  role: z.enum(ROLE_VALUES),
  is_child: z.boolean(),
  disabled: z.boolean(),
  has_password: z.boolean(),
  credential_link_expires_at: z.iso.datetime().nullable(),
  created_at: z.iso.datetime(),
  updated_at: z.iso.datetime(),
});

type User = z.infer<typeof UserSchema>;

const UsersListSchema = z.array(UserSchema);

/** `GET /api/v1/users` — list all users (admin only). */
async function listUsers(signal?: AbortSignal): Promise<User[]> {
  const raw = await apiFetch("/api/v1/users", { signal });
  return UsersListSchema.parse(raw);
}

/** `PUT /api/v1/users/{id}/role` — change a user's role (admin only). */
async function updateUserRole(id: string, role: Role, signal?: AbortSignal): Promise<User> {
  const raw = await apiFetch(`/api/v1/users/${id}/role`, {
    method: "PUT",
    body: JSON.stringify({ role }),
    signal,
  });
  return UserSchema.parse(raw);
}

/** `PUT /api/v1/users/{id}/child-status` — toggle child status (admin only). */
async function updateUserChildStatus(
  id: string,
  isChild: boolean,
  signal?: AbortSignal,
): Promise<User> {
  const raw = await apiFetch(`/api/v1/users/${id}/child-status`, {
    method: "PUT",
    body: JSON.stringify({ is_child: isChild }),
    signal,
  });
  return UserSchema.parse(raw);
}

/** Fields for `POST /api/v1/users` (admin create / invite). */
type CreateUserInput = {
  email: string;
  display_name: string;
  role: Role;
};

const CredentialLinkSchema = z.object({ token: z.string(), expires_at: z.iso.datetime() });
type CredentialLink = z.infer<typeof CredentialLinkSchema>;
const CreatedUserSchema = z.object({ user: UserSchema, link: CredentialLinkSchema });
type CreatedUser = z.infer<typeof CreatedUserSchema>;

async function createUser(input: CreateUserInput, signal?: AbortSignal): Promise<CreatedUser> {
  const raw = await apiFetch("/api/v1/users", {
    method: "POST",
    body: JSON.stringify(input),
    signal,
  });
  return CreatedUserSchema.parse(raw);
}

/** `PUT /api/v1/users/{id}/account-status` — soft-disable or re-enable (admin only). */
async function setAccountStatus(
  id: string,
  disabled: boolean,
  signal?: AbortSignal,
): Promise<User> {
  const raw = await apiFetch(`/api/v1/users/${id}/account-status`, {
    method: "PUT",
    body: JSON.stringify({ disabled }),
    signal,
  });
  return UserSchema.parse(raw);
}

async function issueCredentialLink(id: string): Promise<CredentialLink> {
  return CredentialLinkSchema.parse(
    await apiFetch(`/api/v1/users/${id}/credential-links`, { method: "POST" }),
  );
}

async function cancelCredentialLink(id: string): Promise<void> {
  await apiFetch(`/api/v1/users/${id}/credential-links`, { method: "DELETE" });
}

/** Fields accepted by `PATCH /api/v1/users/{id}`. */
interface UpdateUserFields {
  display_name?: string | null;
  email?: string | null;
}

/** `PATCH /api/v1/users/{id}` — update display name / email (admin only). */
async function updateUser(
  id: string,
  fields: UpdateUserFields,
  signal?: AbortSignal,
): Promise<User> {
  const raw = await apiFetch(`/api/v1/users/${id}`, {
    method: "PATCH",
    body: JSON.stringify(fields),
    signal,
  });
  return UserSchema.parse(raw);
}

export {
  listUsers,
  updateUserRole,
  updateUserChildStatus,
  updateUser,
  createUser,
  setAccountStatus,
  issueCredentialLink,
  cancelCredentialLink,
};
export type { CredentialLink, CreatedUser, User, Role, UpdateUserFields, CreateUserInput };

/**
 * Personal device-token API client (`/api/v1/tokens*`).
 *
 * Self-service: every endpoint operates on the caller's own tokens, scoped
 * by session, not by role. Mirrors the response DTOs in
 * `backend/src/routes/tokens.rs`.
 */
import { z } from "zod";

import { apiFetch } from "./fetch";

const SCOPE_VALUES = ["read", "write", "admin"] as const;
type Scope = (typeof SCOPE_VALUES)[number];

const TokenSchema = z.object({
  id: z.uuid(),
  name: z.string(),
  scopes: z.array(z.enum(SCOPE_VALUES)),
  expires_at: z.string().nullable(),
  last_used_at: z.string().nullable(),
  created_at: z.string(),
});

/** One active device token in the list view (no plaintext, no hash). */
type Token = z.infer<typeof TokenSchema>;

const TokensListSchema = z.array(TokenSchema);

const CreateTokenResponseSchema = TokenSchema.extend({
  /**
   * The full Bearer credential (`{prefix}{id}.{secret}`), present only in
   * this mint response — the plaintext is never recoverable afterwards.
   */
  token: z.string(),
});

/** Mint response: the token row plus its one-time plaintext credential. */
type CreateTokenResponse = z.infer<typeof CreateTokenResponseSchema>;

/** Fields for `POST /api/v1/tokens`. */
type CreateTokenInput = {
  name: string;
  scopes: Scope[];
  /** `null` mints a token that never expires. */
  expiresInDays: number | null;
};

/** `GET /api/v1/tokens` — list the caller's active (non-revoked) tokens. */
async function listTokens(signal?: AbortSignal): Promise<Token[]> {
  const raw = await apiFetch("/api/v1/tokens", { signal });
  return TokensListSchema.parse(raw);
}

/**
 * `POST /api/v1/tokens` — mint a new device token for the caller. The
 * response's `token` field is shown to the user exactly once.
 */
async function createToken(
  input: CreateTokenInput,
  signal?: AbortSignal,
): Promise<CreateTokenResponse> {
  const raw = await apiFetch("/api/v1/tokens", {
    method: "POST",
    body: JSON.stringify({
      name: input.name,
      scopes: input.scopes,
      expires_in_days: input.expiresInDays,
    }),
    signal,
  });
  return CreateTokenResponseSchema.parse(raw);
}

/** `DELETE /api/v1/tokens/{id}` — revoke one of the caller's tokens. */
async function revokeToken(id: string, signal?: AbortSignal): Promise<void> {
  await apiFetch(`/api/v1/tokens/${encodeURIComponent(id)}`, {
    method: "DELETE",
    signal,
  });
}

export { listTokens, createToken, revokeToken, SCOPE_VALUES };
export type { Token, Scope, CreateTokenInput, CreateTokenResponse };

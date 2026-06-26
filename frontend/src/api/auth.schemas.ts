/**
 * Request-field schemas for the `/auth/*` endpoints.
 *
 * Kept separate from `auth.ts` so the auth forms can import these validators
 * without pulling in (or being affected by mocks of) the imperative request
 * wrappers. Per `frontend/CLAUDE.md`, form inputs and request bodies are
 * Zod-parsed before use; the server re-validates as the authority, so these
 * are the client-side UX and defense-in-depth gate.
 */
import { z } from "zod";

/** A syntactically valid email address. */
export const emailField = z.email();
/** A non-empty display name (rejects whitespace-only input). */
export const displayNameField = z.string().trim().min(1);
/** New-password floor mirrors the server default and the form hint (8 chars). */
export const newPasswordField = z.string().min(8);
/** A recovery PIN is opaque here; the server validates its value. */
export const pinField = z.string().trim().min(1);
/** Login takes an existing credential, so no new-password policy applies. */
export const currentPasswordField = z.string().min(1);

/** Body for `POST /auth/local/login`. */
export const LoginLocalSchema = z.object({ email: emailField, password: currentPasswordField });
/** Body for `POST /auth/setup`. */
export const SetupAdminSchema = z.object({
  email: emailField,
  display_name: displayNameField,
  password: newPasswordField,
});
/** Body for `POST /auth/forgot-password`. */
export const ForgotPasswordSchema = z.object({ email: emailField });
/** Body for `POST /auth/reset-password`. */
export const ResetPasswordSchema = z.object({
  email: emailField,
  pin: pinField,
  new_password: newPasswordField,
});

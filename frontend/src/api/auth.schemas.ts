/**
 * Request-field schemas for the `/auth/*` endpoints.
 *
 * Kept separate from `auth.ts` so the auth forms can import these validators
 * without pulling in (or being affected by mocks of) the imperative request
 * wrappers. Form inputs and request bodies are
 * Zod-parsed before use; the server re-validates as the authority, so these
 * are the client-side UX and defense-in-depth gate.
 */
import { z } from "zod";

/** A syntactically valid email address. */
export const emailField = z.email();
/** A non-empty display name (rejects whitespace-only input). */
export const displayNameField = z.string().trim().min(1);
export type PasswordLengthPolicy = {
  password_min_length: number;
  password_max_length: number;
};

export function newPasswordField(policy: PasswordLengthPolicy): z.ZodString {
  return z.string().superRefine((value, ctx) => {
    const length = Array.from(value).length;
    if (length > policy.password_max_length) {
      ctx.addIssue({
        code: "custom",
        message: `Use at most ${String(policy.password_max_length)} characters.`,
      });
    } else if (length < policy.password_min_length) {
      ctx.addIssue({
        code: "custom",
        message: `Use at least ${String(policy.password_min_length)} characters.`,
      });
    }
  });
}
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
  password: z.string().min(1),
});
/** Body for `POST /auth/forgot-password`. */
export const ForgotPasswordSchema = z.object({ email: emailField });
/** Body for `POST /auth/reset-password`. */
export const ResetPasswordSchema = z.object({
  email: emailField,
  pin: pinField,
  new_password: z.string().min(1),
});
/** Body for `POST /auth/register` (self-service registration). */
export const RegisterSchema = z.object({
  email: emailField,
  display_name: displayNameField,
  password: z.string().min(1),
});
/** Body for `POST /api/v1/account/password` (self-service change). */
export const ChangePasswordSchema = z.object({
  current_password: currentPasswordField,
  new_password: z.string().min(1),
});

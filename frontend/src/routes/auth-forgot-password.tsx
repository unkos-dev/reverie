/**
 * `/forgot-password` — email-less PIN recovery.
 *
 * Two phases. The request phase posts the email and always returns a
 * generic success (no account enumeration); the operator reads the
 * issued PIN from the server host file and hands it to the user. The
 * reset phase takes that PIN plus a new password. The carried email
 * drives the second phase, so it lives in component state while the rest
 * of each form stays uncontrolled (`FormData`). Reset does not establish
 * a session, so on success it routes to the login screen.
 */
import { useMutation } from "@tanstack/react-query";
import { useState, type ReactElement, type SyntheticEvent } from "react";
import { Link, useNavigate } from "react-router";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { requestPasswordReset, resetPassword } from "@/api/auth";
import { emailField, newPasswordField, pinField } from "@/api/auth.schemas";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formString } from "@/lib/form";

import { AuthShell } from "./auth-shell";

const RECOVERY_NOTICE =
  "If that account exists, a recovery PIN was written to the server. Enter it below with a new password.";

/** Route component for `/forgot-password`. */
export function Component(): ReactElement {
  const navigate = useNavigate();
  const [email, setEmail] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const requestMutation = useMutation({
    mutationFn: (submitted: string) => requestPasswordReset(submitted),
    onSuccess: (_data, submitted) => {
      setEmail(submitted);
      toast.info(RECOVERY_NOTICE);
    },
    onError: (err) => {
      const message =
        err instanceof ApiError && err.detail !== "" ? err.detail : "Could not start recovery.";
      setError(message);
      toast.error(message);
    },
  });

  const resetMutation = useMutation({
    mutationFn: ({ pin, newPassword }: { pin: string; newPassword: string }) =>
      resetPassword(email ?? "", pin, newPassword),
    onSuccess: () => {
      toast.success("Password reset. Sign in with your new password.");
      void navigate("/login");
    },
    onError: (err) => {
      const message =
        err instanceof ApiError && err.detail !== "" ? err.detail : "Could not reset the password.";
      setError(message);
      toast.error(message);
    },
  });

  function handleRequest(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    setError(null);
    const data = new FormData(e.currentTarget);
    const parsed = emailField.safeParse(formString(data, "email"));
    if (!parsed.success) {
      setError("Enter a valid email address.");
      return;
    }
    requestMutation.mutate(parsed.data);
  }

  function handleReset(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    setError(null);
    const data = new FormData(e.currentTarget);
    const pin = pinField.safeParse(formString(data, "pin"));
    if (!pin.success) {
      setError("Enter the recovery PIN.");
      return;
    }
    const newPassword = newPasswordField.safeParse(formString(data, "new_password"));
    if (!newPassword.success) {
      setError("Use at least 8 characters for the new password.");
      return;
    }
    resetMutation.mutate({ pin: pin.data, newPassword: newPassword.data });
  }

  if (email === null) {
    return (
      <AuthShell
        title="Forgot password"
        description="Enter your email to start recovery."
        footer={<BackToSignIn />}
      >
        <form key="request" onSubmit={handleRequest} className="flex flex-col gap-4" noValidate>
          <Field>
            <FieldLabel htmlFor="email">Email</FieldLabel>
            <Input
              id="email"
              name="email"
              type="email"
              autoComplete="email"
              required
              aria-invalid={error !== null || undefined}
            />
          </Field>
          {error !== null ? <FieldError>{error}</FieldError> : null}
          <Button type="submit" disabled={requestMutation.isPending}>
            Send recovery PIN
          </Button>
        </form>
      </AuthShell>
    );
  }

  return (
    <AuthShell title="Enter recovery PIN" description={RECOVERY_NOTICE} footer={<BackToSignIn />}>
      <form key="reset" onSubmit={handleReset} className="flex flex-col gap-4" noValidate>
        <Field>
          <FieldLabel htmlFor="pin">Recovery PIN</FieldLabel>
          <Input
            id="pin"
            name="pin"
            type="text"
            inputMode="numeric"
            autoComplete="one-time-code"
            required
            aria-invalid={error !== null || undefined}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor="new_password">New password</FieldLabel>
          <Input
            id="new_password"
            name="new_password"
            type="password"
            autoComplete="new-password"
            required
            aria-invalid={error !== null || undefined}
          />
          <FieldDescription>
            Use at least 8 characters. Avoid common words or passwords from known data breaches.
          </FieldDescription>
        </Field>
        {error !== null ? <FieldError>{error}</FieldError> : null}
        <Button type="submit" disabled={resetMutation.isPending}>
          Reset password
        </Button>
      </form>
    </AuthShell>
  );
}

function BackToSignIn(): ReactElement {
  return (
    <Link to="/login" className="hover:text-fg underline underline-offset-4">
      Back to sign in
    </Link>
  );
}

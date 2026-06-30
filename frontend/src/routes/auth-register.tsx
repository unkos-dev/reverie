/**
 * `/register`: self-service account registration.
 *
 * NOTE: intentionally not routed in `main.tsx`. Registration today creates an
 * immediately-active account with no admin approval, so account creation stays
 * admin-provisioned and this screen is dormant. It is kept here (and tested) as
 * the starting point for an approval-gated request-access flow. Wire the route
 * up only once registration is moderated.
 *
 * Config-gated on the server (`self_registration_enabled`); a 404 means the
 * instance has it turned off, surfaced inline. A self-registered account is
 * always an adult. Registration does not establish a session (parity with
 * setup and recovery), so on success it routes to the login screen.
 *
 * Forms are uncontrolled (`FormData`) per `frontend/CLAUDE.md`.
 */
import { useMutation } from "@tanstack/react-query";
import { useState, type ReactElement, type SyntheticEvent } from "react";
import { Link, useNavigate } from "react-router";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { register } from "@/api/auth";
import { displayNameField, emailField, newPasswordField } from "@/api/auth.schemas";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formString } from "@/lib/form";

import { AuthShell } from "./auth-shell";

/** Route component for `/register`. */
export function Component(): ReactElement {
  const navigate = useNavigate();
  const [error, setError] = useState<string | null>(null);

  const registerMutation = useMutation({
    mutationFn: ({
      email,
      displayName,
      password,
    }: {
      email: string;
      displayName: string;
      password: string;
    }) => register(email, displayName, password),
    onSuccess: () => {
      toast.success("Account created. Sign in to continue.");
      void navigate("/login");
    },
    onError: (err) => {
      const message =
        err instanceof ApiError && err.detail !== "" ? err.detail : "Could not create the account.";
      setError(message);
      toast.error(message);
    },
  });

  function handleSubmit(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    setError(null);
    const data = new FormData(e.currentTarget);
    const displayName = displayNameField.safeParse(formString(data, "display_name"));
    if (!displayName.success) {
      setError("Enter a display name.");
      return;
    }
    const email = emailField.safeParse(formString(data, "email"));
    if (!email.success) {
      setError("Enter a valid email address.");
      return;
    }
    const password = newPasswordField.safeParse(formString(data, "password"));
    if (!password.success) {
      setError("Use at least 8 characters for the password.");
      return;
    }
    registerMutation.mutate({
      email: email.data,
      displayName: displayName.data,
      password: password.data,
    });
  }

  return (
    <AuthShell
      title="Create your account"
      description="Register to start using this instance."
      footer={
        <Link to="/login" className="hover:text-fg underline underline-offset-4">
          Already have an account? Sign in
        </Link>
      }
    >
      <form onSubmit={handleSubmit} className="flex flex-col gap-4" noValidate>
        <Field>
          <FieldLabel htmlFor="display_name">Display name</FieldLabel>
          <Input
            id="display_name"
            name="display_name"
            type="text"
            autoComplete="name"
            required
            aria-invalid={error !== null || undefined}
          />
        </Field>
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
        <Field>
          <FieldLabel htmlFor="password">Password</FieldLabel>
          <Input
            id="password"
            name="password"
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
        <Button type="submit" disabled={registerMutation.isPending}>
          Create account
        </Button>
      </form>
    </AuthShell>
  );
}

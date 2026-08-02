/**
 * `/account/password`: self-service password change for the signed-in user.
 *
 * Verifies the current password and sets a new one. The server invalidates
 * every session on success, so this routes to the login screen afterwards
 * rather than keeping a now-dead session in the UI.
 *
 * Forms are uncontrolled (`FormData`).
 */
import { useMutation } from "@tanstack/react-query";
import { useState, type ReactElement, type SyntheticEvent } from "react";
import { Link, useNavigate } from "react-router";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { changeOwnPassword } from "@/api/auth";
import { currentPasswordField, newPasswordField } from "@/api/auth.schemas";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formString } from "@/lib/form";

import { AuthShell } from "./auth-shell";

/**
 * A validation error tagged with the field it belongs to, so `aria-invalid` is
 * scoped to the failing input. A server rejection covers more than one field
 * (wrong current password or a rejected new one), so it stays `form` level and
 * is announced through the alert without mislabelling a specific input.
 */
type FormError = { field: "current" | "new" | "form"; message: string };

/** Route component for `/account/password`. */
export function Component(): ReactElement {
  const navigate = useNavigate();
  const [error, setError] = useState<FormError | null>(null);

  const changeMutation = useMutation({
    mutationFn: ({
      currentPassword,
      newPassword,
    }: {
      currentPassword: string;
      newPassword: string;
    }) => changeOwnPassword(currentPassword, newPassword),
    onSuccess: () => {
      toast.success("Password changed. Sign in with your new password.");
      void navigate("/login");
    },
    onError: (err) => {
      const message =
        err instanceof ApiError && err.detail !== ""
          ? err.detail
          : "Could not change the password.";
      setError({ field: "form", message });
      toast.error(message);
    },
  });

  function handleSubmit(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    setError(null);
    const data = new FormData(e.currentTarget);
    const currentPassword = currentPasswordField.safeParse(formString(data, "current_password"));
    if (!currentPassword.success) {
      setError({ field: "current", message: "Enter your current password." });
      return;
    }
    const newPassword = newPasswordField.safeParse(formString(data, "new_password"));
    if (!newPassword.success) {
      setError({
        field: "new",
        message: "Use at least 8 characters for the new password.",
      });
      return;
    }
    changeMutation.mutate({
      currentPassword: currentPassword.data,
      newPassword: newPassword.data,
    });
  }

  return (
    <AuthShell
      title="Change password"
      description="Set a new password. You will sign in again afterwards."
      footer={
        <Link to="/library" className="hover:text-fg underline underline-offset-4">
          Back to library
        </Link>
      }
    >
      <form onSubmit={handleSubmit} className="flex flex-col gap-4" noValidate>
        <Field>
          <FieldLabel htmlFor="current_password">Current password</FieldLabel>
          <Input
            id="current_password"
            name="current_password"
            type="password"
            autoComplete="current-password"
            required
            aria-invalid={error?.field === "current" || undefined}
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
            aria-invalid={error?.field === "new" || undefined}
          />
          <FieldDescription>
            Use at least 8 characters. Avoid common words or passwords from known data breaches.
          </FieldDescription>
        </Field>
        {error ? <FieldError>{error.message}</FieldError> : null}
        <Button type="submit" disabled={changeMutation.isPending}>
          Change password
        </Button>
      </form>
    </AuthShell>
  );
}

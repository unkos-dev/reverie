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
import {
  FieldError as AriaFieldError,
  Form,
  Input as AriaInput,
  Label as AriaLabel,
  TextField,
} from "react-aria-components";
import { Link, useNavigate } from "react-router";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { changeOwnPassword } from "@/api/auth";
import { currentPasswordField } from "@/api/auth.schemas";
import { PasswordPolicyHint } from "@/components/PasswordPolicyHint";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { usePasswordPolicy } from "@/hooks/usePasswordPolicy";
import { formString } from "@/lib/form";

import { AuthShell } from "./auth-shell";

function validateCurrentPassword(value: string): string | null {
  return currentPasswordField.safeParse(value).success ? null : "Enter your current password.";
}

/** Route component for `/account/password`. */
export function Component(): ReactElement {
  const { policy, ready, isError: policyError, validate } = usePasswordPolicy();
  const navigate = useNavigate();
  const [error, setError] = useState<string | null>(null);

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
      setError(message);
      toast.error(message);
    },
  });

  function handleSubmit(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    if (!ready) return;
    setError(null);
    const data = new FormData(e.currentTarget);
    const currentPassword = currentPasswordField.safeParse(formString(data, "current_password"));
    const newPassword = formString(data, "new_password");
    if (!currentPassword.success || validate(newPassword) !== undefined) return;
    changeMutation.mutate({
      currentPassword: currentPassword.data,
      newPassword: newPassword,
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
      <Form onSubmit={handleSubmit} className="flex flex-col gap-4">
        <TextField
          name="current_password"
          type="password"
          autoComplete="current-password"
          isRequired
          validate={validateCurrentPassword}
          render={(props) => <Field {...props} />}
        >
          <AriaLabel render={(props) => <FieldLabel {...props} />}>Current password</AriaLabel>
          <AriaInput render={(props) => <Input {...props} />} />
          <AriaFieldError className="text-destructive text-sm font-normal" />
        </TextField>
        <TextField
          name="new_password"
          type="password"
          autoComplete="new-password"
          isRequired
          validate={(value) => (ready ? (validate(value) ?? null) : null)}
          render={(props) => <Field {...props} />}
        >
          <AriaLabel render={(props) => <FieldLabel {...props} />}>New password</AriaLabel>
          <AriaInput render={(props) => <Input {...props} />} />
          <PasswordPolicyHint policy={policy} failed={policyError} />
          <AriaFieldError className="text-destructive text-sm font-normal" />
        </TextField>
        {error !== null ? <FieldError>{error}</FieldError> : null}
        <Button type="submit" isPending={changeMutation.isPending} isDisabled={!ready}>
          Change password
        </Button>
      </Form>
    </AuthShell>
  );
}

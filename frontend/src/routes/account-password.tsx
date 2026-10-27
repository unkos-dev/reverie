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
import { useActionState, type ReactElement } from "react";
import { Form, type FormProps } from "react-aria-components";
import { Link, useNavigate } from "react-router";
import { z } from "zod";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { changeOwnPassword } from "@/api/auth";
import { ChangePasswordSchema, currentPasswordField } from "@/api/auth.schemas";
import { PasswordPolicyHint } from "@/components/PasswordPolicyHint";
import { Button } from "@/components/ui/button";
import { FieldError, TextInputField } from "@/components/ui/field";
import { usePasswordPolicy } from "@/hooks/usePasswordPolicy";

import { AuthShell } from "./auth-shell";

function validateCurrentPassword(value: string): string | null {
  return currentPasswordField.safeParse(value).success ? null : "Enter your current password.";
}

/** Route component for `/account/password`. */
export function Component(): ReactElement {
  const { policy, ready, isError: policyError, validate } = usePasswordPolicy();
  const navigate = useNavigate();

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
  });

  const [state, action, pending] = useActionState<
    {
      values?: { current_password: string; new_password: string };
      errors?: FormProps["validationErrors"];
      error?: string;
    },
    FormData
  >(async (_previous, data) => {
    const values = z
      .object({ current_password: z.string(), new_password: z.string() })
      .parse(Object.fromEntries(data));
    const newPasswordError = validate(values.new_password);
    if (!ready) return { values };
    if (newPasswordError !== undefined)
      return { values, errors: { new_password: newPasswordError } };
    const parsed = ChangePasswordSchema.safeParse(values);
    if (!parsed.success) return { values, errors: z.flattenError(parsed.error).fieldErrors };
    try {
      await changeMutation.mutateAsync({
        currentPassword: parsed.data.current_password,
        newPassword: parsed.data.new_password,
      });
      return {};
    } catch (err) {
      const message =
        err instanceof ApiError && err.detail !== ""
          ? err.detail
          : "Could not change the password.";
      toast.error(message);
      return { values, error: message };
    }
  }, {});

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
      <Form action={action} validationErrors={state.errors} className="flex flex-col gap-4">
        <TextInputField
          name="current_password"
          defaultValue={state.values?.current_password}
          type="password"
          autoComplete="current-password"
          isRequired
          validate={validateCurrentPassword}
          label="Current password"
        />
        <TextInputField
          name="new_password"
          defaultValue={state.values?.new_password}
          type="password"
          autoComplete="new-password"
          isRequired
          validate={(value) => (ready ? (validate(value) ?? null) : null)}
          label="New password"
        >
          <PasswordPolicyHint policy={policy} failed={policyError} />
        </TextInputField>
        {!pending && state.error !== undefined ? <FieldError>{state.error}</FieldError> : null}
        <Button type="submit" isPending={pending} isDisabled={!ready}>
          Change password
        </Button>
      </Form>
    </AuthShell>
  );
}

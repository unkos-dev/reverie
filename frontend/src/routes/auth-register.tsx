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
 * Forms are uncontrolled (`FormData`).
 */
import { useMutation } from "@tanstack/react-query";
import { useState, type ReactElement, type SyntheticEvent } from "react";
import { Form } from "react-aria-components";
import { Link, useNavigate } from "react-router";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { register } from "@/api/auth";
import { displayNameField, emailField } from "@/api/auth.schemas";
import { PasswordPolicyHint } from "@/components/PasswordPolicyHint";
import { Button } from "@/components/ui/button";
import { FieldError, TextInputField } from "@/components/ui/field";
import { usePasswordPolicy } from "@/hooks/usePasswordPolicy";
import { formString } from "@/lib/form";

import { AuthShell } from "./auth-shell";

function validateDisplayName(value: string): string | null {
  return displayNameField.safeParse(value).success ? null : "Enter a display name.";
}

function validateEmail(value: string): string | null {
  return emailField.safeParse(value).success ? null : "Enter a valid email address.";
}

/** Route component for `/register`. */
export function Component(): ReactElement {
  const { policy, ready, isError: policyError, validate } = usePasswordPolicy();
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
    if (!ready) return;
    setError(null);
    const data = new FormData(e.currentTarget);
    const displayName = displayNameField.safeParse(formString(data, "display_name"));
    const email = emailField.safeParse(formString(data, "email"));
    const password = formString(data, "password");
    if (!displayName.success || !email.success || validate(password) !== undefined) return;
    registerMutation.mutate({
      email: email.data,
      displayName: displayName.data,
      password: password,
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
      <Form onSubmit={handleSubmit} className="flex flex-col gap-4">
        <TextInputField
          name="display_name"
          type="text"
          autoComplete="name"
          isRequired
          validate={validateDisplayName}
          label="Display name"
        />
        <TextInputField
          name="email"
          type="email"
          autoComplete="email"
          isRequired
          validate={validateEmail}
          label="Email"
        />
        <TextInputField
          name="password"
          type="password"
          autoComplete="new-password"
          isRequired
          validate={(value) => (ready ? (validate(value) ?? null) : null)}
          label="Password"
        >
          <PasswordPolicyHint policy={policy} failed={policyError} />
        </TextInputField>
        {error !== null ? <FieldError>{error}</FieldError> : null}
        <Button type="submit" isPending={registerMutation.isPending} isDisabled={!ready}>
          Create account
        </Button>
      </Form>
    </AuthShell>
  );
}

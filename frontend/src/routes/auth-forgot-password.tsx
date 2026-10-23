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
import { Form } from "react-aria-components";
import { Link, useNavigate } from "react-router";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { requestPasswordReset, resetPassword } from "@/api/auth";
import { emailField, pinField } from "@/api/auth.schemas";
import { PasswordPolicyHint } from "@/components/PasswordPolicyHint";
import { Button } from "@/components/ui/button";
import { FieldError, TextInputField } from "@/components/ui/field";
import { usePasswordPolicy } from "@/hooks/usePasswordPolicy";
import { formString } from "@/lib/form";

import { AuthShell } from "./auth-shell";

const RECOVERY_NOTICE =
  "If that account exists, a recovery PIN was written to the server. Enter it below with a new password.";

function validateEmail(value: string): string | null {
  return emailField.safeParse(value).success ? null : "Enter a valid email address.";
}

function validatePin(value: string): string | null {
  return pinField.safeParse(value).success ? null : "Enter the recovery PIN.";
}

/** Route component for `/forgot-password`. */
export function Component(): ReactElement {
  const { policy, ready, isError: policyError, validate } = usePasswordPolicy();
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
    if (requestMutation.isPending) return;
    setError(null);
    const data = new FormData(e.currentTarget);
    const parsed = emailField.safeParse(formString(data, "email"));
    if (!parsed.success) return;
    requestMutation.mutate(parsed.data);
  }

  function handleReset(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    if (!ready) return;
    setError(null);
    const data = new FormData(e.currentTarget);
    const pin = pinField.safeParse(formString(data, "pin"));
    const newPassword = formString(data, "new_password");
    if (!pin.success || validate(newPassword) !== undefined) return;
    resetMutation.mutate({ pin: pin.data, newPassword: newPassword });
  }

  if (email === null) {
    return (
      <AuthShell
        title="Forgot password"
        description="Enter your email to start recovery."
        footer={<BackToSignIn />}
      >
        <Form key="request" onSubmit={handleRequest} className="flex flex-col gap-4">
          <TextInputField
            name="email"
            type="email"
            autoComplete="email"
            isRequired
            validate={validateEmail}
            label="Email"
          />
          {error !== null ? <FieldError>{error}</FieldError> : null}
          <Button type="submit" isPending={requestMutation.isPending}>
            Send recovery PIN
          </Button>
        </Form>
      </AuthShell>
    );
  }

  return (
    <AuthShell title="Enter recovery PIN" description={RECOVERY_NOTICE} footer={<BackToSignIn />}>
      <Form key="reset" onSubmit={handleReset} className="flex flex-col gap-4">
        <TextInputField
          name="pin"
          type="text"
          inputMode="numeric"
          autoComplete="one-time-code"
          isRequired
          validate={validatePin}
          label="Recovery PIN"
        />
        <TextInputField
          name="new_password"
          type="password"
          autoComplete="new-password"
          isRequired
          validate={(value) => (ready ? (validate(value) ?? null) : null)}
          label="New password"
        >
          <PasswordPolicyHint policy={policy} failed={policyError} />
        </TextInputField>
        {error !== null ? <FieldError>{error}</FieldError> : null}
        <Button type="submit" isPending={resetMutation.isPending} isDisabled={!ready}>
          Reset password
        </Button>
      </Form>
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

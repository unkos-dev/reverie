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
import { requestPasswordReset, resetPassword } from "@/api/auth";
import { emailField, pinField } from "@/api/auth.schemas";
import { PasswordPolicyHint } from "@/components/PasswordPolicyHint";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
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
          <TextField
            name="email"
            type="email"
            autoComplete="email"
            isRequired
            validate={validateEmail}
            render={(props) => <Field {...props} />}
          >
            <AriaLabel render={(props) => <FieldLabel {...props} />}>Email</AriaLabel>
            <AriaInput render={(props) => <Input {...props} />} />
            <AriaFieldError className="text-destructive text-sm font-normal" />
          </TextField>
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
        <TextField
          name="pin"
          type="text"
          inputMode="numeric"
          autoComplete="one-time-code"
          isRequired
          validate={validatePin}
          render={(props) => <Field {...props} />}
        >
          <AriaLabel render={(props) => <FieldLabel {...props} />}>Recovery PIN</AriaLabel>
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

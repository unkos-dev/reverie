/**
 * `/setup` — first-run administrator bootstrap.
 *
 * The only page that mints the first administrator (the bootstrap API is
 * `POST /auth/setup`). Shown when no administrator exists yet
 * (`setup_required`); once the instance is initialized it bounces back to
 * `/login` so a direct visit cannot present a form that would only 409 on
 * submit. Setup does not establish a session (parity with recovery), so on
 * success it routes to the login screen to sign in.
 *
 * Forms are uncontrolled (`FormData`).
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type ReactElement, type SyntheticEvent } from "react";
import {
  FieldError as AriaFieldError,
  Form,
  Input as AriaInput,
  Label as AriaLabel,
  TextField,
} from "react-aria-components";
import { Navigate, useNavigate } from "react-router";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { fetchSetupStatus, setupAdmin, type SetupStatus } from "@/api/auth";
import { displayNameField, emailField, newPasswordField } from "@/api/auth.schemas";
import { Button } from "@/components/ui/button";
import { PasswordPolicyHint } from "@/components/PasswordPolicyHint";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formString } from "@/lib/form";
import { queryKeys } from "@/lib/query/keys";

import { AuthShell } from "./auth-shell";

/** Route component for `/setup`. */
export function Component(): ReactElement {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const {
    data: status,
    isPending,
    isError,
  } = useQuery({
    queryKey: queryKeys.auth.setupStatus(),
    queryFn: ({ signal }) => fetchSetupStatus(signal),
    retry: false,
    staleTime: Infinity,
  });

  const [error, setError] = useState<string | null>(null);

  const setupMutation = useMutation({
    mutationFn: ({
      email,
      displayName,
      password,
    }: {
      email: string;
      displayName: string;
      password: string;
    }) => setupAdmin(email, displayName, password),
    onSuccess: () => {
      queryClient.setQueryData<SetupStatus>(queryKeys.auth.setupStatus(), (old) =>
        old !== undefined ? { ...old, setup_required: false } : old,
      );
      toast.success("Administrator created. Sign in to continue.");
      void navigate("/login");
    },
    onError: (err) => {
      const message =
        err instanceof ApiError && err.detail !== "" ? err.detail : "Could not complete setup.";
      setError(message);
      toast.error(message);
    },
  });

  if (isPending) {
    return (
      <AuthShell title="Set up Reverie">
        <p className="text-fg-muted text-sm">Loading…</p>
      </AuthShell>
    );
  }

  if (isError) {
    return (
      <AuthShell title="Set up Reverie">
        <p className="text-fg-muted text-sm" role="alert">
          We couldn&apos;t load setup. Try reloading the page.
        </p>
      </AuthShell>
    );
  }

  if (!status.setup_required) {
    return <Navigate to="/login" replace />;
  }

  const passwordField = newPasswordField(status);

  function validateDisplayName(value: string): string | null {
    return displayNameField.safeParse(value).success ? null : "Enter a display name.";
  }

  function validateEmail(value: string): string | null {
    return emailField.safeParse(value).success ? null : "Enter a valid email address.";
  }

  function validatePassword(value: string): string | null {
    const parsed = passwordField.safeParse(value);
    return parsed.success
      ? null
      : (parsed.error.issues[0]?.message ?? "Check the password length.");
  }

  function handleSubmit(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    setError(null);
    const data = new FormData(e.currentTarget);
    const displayName = displayNameField.safeParse(formString(data, "display_name"));
    const email = emailField.safeParse(formString(data, "email"));
    const password = passwordField.safeParse(formString(data, "password"));
    if (!displayName.success || !email.success || !password.success) return;
    setupMutation.mutate({
      email: email.data,
      displayName: displayName.data,
      password: password.data,
    });
  }

  return (
    <AuthShell
      title="Set up Reverie"
      description="Create the first administrator account for this instance."
    >
      <Form onSubmit={handleSubmit} className="flex flex-col gap-4">
        <TextField
          name="display_name"
          type="text"
          autoComplete="name"
          isRequired
          validate={validateDisplayName}
          render={(props) => <Field {...props} />}
        >
          <AriaLabel render={(props) => <FieldLabel {...props} />}>Display name</AriaLabel>
          <AriaInput render={(props) => <Input {...props} />} />
          <AriaFieldError className="text-destructive text-sm font-normal" />
        </TextField>
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
        <TextField
          name="password"
          type="password"
          autoComplete="new-password"
          isRequired
          validate={validatePassword}
          render={(props) => <Field {...props} />}
        >
          <AriaLabel render={(props) => <FieldLabel {...props} />}>Password</AriaLabel>
          <AriaInput render={(props) => <Input {...props} />} />
          <PasswordPolicyHint policy={status} failed={false} />
          <AriaFieldError className="text-destructive text-sm font-normal" />
        </TextField>
        {error !== null ? <FieldError>{error}</FieldError> : null}
        <Button type="submit" isPending={setupMutation.isPending}>
          Create administrator
        </Button>
      </Form>
    </AuthShell>
  );
}

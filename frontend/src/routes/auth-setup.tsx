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
import { useActionState, type ReactElement } from "react";
import { Form, type FormProps } from "react-aria-components";
import { Navigate, useNavigate } from "react-router";
import { z } from "zod";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { fetchSetupStatus, setupAdmin, type SetupStatus } from "@/api/auth";
import {
  SetupAdminSchema,
  newPasswordField,
  validateDisplayName,
  validateEmail,
} from "@/api/auth.schemas";
import { Button } from "@/components/ui/button";
import { PasswordPolicyHint } from "@/components/PasswordPolicyHint";
import { FieldError, TextInputField } from "@/components/ui/field";
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
  });

  const [state, action, pending] = useActionState<
    {
      values?: { display_name: string; email: string; password: string };
      errors?: FormProps["validationErrors"];
      error?: string;
    },
    FormData
  >(async (_previous, data) => {
    const values = z
      .object({ display_name: z.string(), email: z.string(), password: z.string() })
      .parse(Object.fromEntries(data));
    if (status === undefined) return { values };
    const password = newPasswordField(status).safeParse(values.password);
    if (!password.success)
      return { values, errors: { password: password.error.issues.map((issue) => issue.message) } };
    const parsed = SetupAdminSchema.safeParse(values);
    if (!parsed.success) return { values, errors: z.flattenError(parsed.error).fieldErrors };
    try {
      await setupMutation.mutateAsync({
        email: parsed.data.email,
        displayName: parsed.data.display_name,
        password: parsed.data.password,
      });
      return {};
    } catch (err) {
      const message =
        err instanceof ApiError && err.detail !== "" ? err.detail : "Could not complete setup.";
      toast.error(message);
      return { values, error: message };
    }
  }, {});

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

  function validatePassword(value: string): string | null {
    const parsed = passwordField.safeParse(value);
    return parsed.success
      ? null
      : (parsed.error.issues[0]?.message ?? "Check the password length.");
  }

  return (
    <AuthShell
      title="Set up Reverie"
      description="Create the first administrator account for this instance."
    >
      <Form action={action} validationErrors={state.errors} className="flex flex-col gap-4">
        <TextInputField
          name="display_name"
          defaultValue={state.values?.display_name}
          type="text"
          autoComplete="name"
          isRequired
          validate={validateDisplayName}
          label="Display name"
        />
        <TextInputField
          name="email"
          defaultValue={state.values?.email}
          type="email"
          autoComplete="email"
          isRequired
          validate={validateEmail}
          label="Email"
        />
        <TextInputField
          name="password"
          defaultValue={state.values?.password}
          type="password"
          autoComplete="new-password"
          isRequired
          validate={validatePassword}
          label="Password"
        >
          <PasswordPolicyHint policy={status} failed={false} />
        </TextInputField>
        {!pending && state.error !== undefined ? <FieldError>{state.error}</FieldError> : null}
        <Button type="submit" isPending={pending}>
          Create administrator
        </Button>
      </Form>
    </AuthShell>
  );
}

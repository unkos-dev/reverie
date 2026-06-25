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
 * Forms are uncontrolled (`FormData`) per `frontend/CLAUDE.md`.
 */
import { useMutation, useQuery } from "@tanstack/react-query";
import { useState, type ReactElement, type SyntheticEvent } from "react";
import { Navigate, useNavigate } from "react-router";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { fetchSetupStatus, setupAdmin } from "@/api/auth";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formString } from "@/lib/form";
import { queryKeys } from "@/lib/query/keys";

import { AuthShell } from "./auth-shell";

/** Route component for `/setup`. */
export function Component(): ReactElement {
  const navigate = useNavigate();
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

  function handleSubmit(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    setError(null);
    const data = new FormData(e.currentTarget);
    setupMutation.mutate({
      email: formString(data, "email"),
      displayName: formString(data, "display_name"),
      password: formString(data, "password"),
    });
  }

  return (
    <AuthShell
      title="Set up Reverie"
      description="Create the first administrator account for this instance."
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
          <FieldDescription>Use at least 8 characters.</FieldDescription>
        </Field>
        {error !== null ? <FieldError>{error}</FieldError> : null}
        <Button type="submit" disabled={setupMutation.isPending}>
          Create administrator
        </Button>
      </form>
    </AuthShell>
  );
}

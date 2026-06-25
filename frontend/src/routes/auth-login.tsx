/**
 * `/auth/login` — local credential sign-in (S2).
 *
 * Replaces the backend OIDC initiator that used to own this path. Reads
 * provider state from `GET /auth/setup/status`: shows the local
 * email/password form when local auth is enabled, a "Sign in with OIDC"
 * action when OIDC is configured, and bounces to `/auth/setup` on a
 * fresh instance with no administrator yet.
 *
 * Forms are uncontrolled (`FormData`) per `frontend/CLAUDE.md`; the plan
 * snippet's controlled inputs are a conscious deviation in favour of the
 * binding doc. On success the local login establishes a session (CSRF
 * token minted server-side, hydrated by `loginLocal`), so the redirect
 * to `/library` is a full-page navigation that reloads with the live
 * session rather than a client-side route change.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type ReactElement, type SyntheticEvent } from "react";
import { Link, Navigate } from "react-router";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { fetchSetupStatus, loginLocal } from "@/api/auth";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formString } from "@/lib/form";
import { queryKeys } from "@/lib/query/keys";

import { AuthShell } from "./auth-shell";

/** Route component for `/auth/login`. */
export function Component(): ReactElement {
  const qc = useQueryClient();
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

  const loginMutation = useMutation({
    mutationFn: ({ email, password }: { email: string; password: string }) =>
      loginLocal(email, password),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.auth.me() });
      window.location.assign("/library");
    },
    onError: (err) => {
      const message =
        err instanceof ApiError && err.detail !== "" ? err.detail : "Incorrect email or password.";
      setError(message);
      toast.error(message);
    },
  });

  if (isPending) {
    return (
      <AuthShell title="Sign in">
        <p className="text-fg-muted text-sm">Loading…</p>
      </AuthShell>
    );
  }

  if (isError) {
    return (
      <AuthShell title="Sign in">
        <p className="text-fg-muted text-sm" role="alert">
          We couldn&apos;t load the sign-in options. Try reloading the page.
        </p>
      </AuthShell>
    );
  }

  if (status.setup_required) {
    return <Navigate to="/auth/setup" replace />;
  }

  function handleSubmit(e: SyntheticEvent<HTMLFormElement>): void {
    e.preventDefault();
    setError(null);
    const data = new FormData(e.currentTarget);
    loginMutation.mutate({
      email: formString(data, "email"),
      password: formString(data, "password"),
    });
  }

  const localForm = status.local_auth_enabled ? (
    <form onSubmit={handleSubmit} className="flex flex-col gap-4" noValidate>
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
          autoComplete="current-password"
          required
          aria-invalid={error !== null || undefined}
        />
      </Field>
      {error !== null ? <FieldError>{error}</FieldError> : null}
      <Button type="submit" disabled={loginMutation.isPending}>
        Sign in
      </Button>
    </form>
  ) : null;

  const oidcAction = status.oidc_enabled ? (
    <Button
      type="button"
      variant="outline"
      onClick={() => {
        window.location.assign("/auth/oidc/login");
      }}
    >
      Sign in with OIDC
    </Button>
  ) : null;

  const divider =
    status.local_auth_enabled && status.oidc_enabled ? (
      <div className="text-fg-muted text-center text-xs">or</div>
    ) : null;

  return (
    <AuthShell
      title="Sign in"
      footer={
        status.local_auth_enabled ? (
          <Link to="/auth/forgot-password" className="hover:text-fg underline underline-offset-4">
            Forgot password?
          </Link>
        ) : undefined
      }
    >
      <div className="flex flex-col gap-4">
        {localForm}
        {divider}
        {oidcAction}
      </div>
    </AuthShell>
  );
}

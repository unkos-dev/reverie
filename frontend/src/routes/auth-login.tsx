/**
 * `/login` — local credential sign-in.
 *
 * A user-facing page (the auth API/protocol lives under `/auth/*`). Reads
 * provider state from `GET /auth/setup/status`: shows the local
 * email/password form when local auth is enabled, a "Sign in with OIDC"
 * action when OIDC is configured, and bounces to `/setup` on a fresh
 * instance with no administrator yet.
 *
 * Forms are uncontrolled (`FormData`). On success
 * the local login establishes a session (CSRF token minted server-side,
 * hydrated by `loginLocal`), so the redirect to `/library` is a full-page
 * navigation that reloads with the live session rather than a client-side
 * route change.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ReactElement, type SyntheticEvent } from "react";
import {
  FieldError as AriaFieldError,
  Form,
  Input as AriaInput,
  Label as AriaLabel,
  TextField,
} from "react-aria-components";
import { Link, Navigate, useLocation } from "react-router";
import { z } from "zod";
import { toast } from "sonner";

import { ApiError } from "@/api";
import { fetchSetupStatus, loginLocal } from "@/api/auth";
import { currentPasswordField, emailField } from "@/api/auth.schemas";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formString } from "@/lib/form";
import { queryKeys } from "@/lib/query/keys";

import { AuthShell } from "./auth-shell";

const outcomeSchema = z.enum(["denied", "expired", "unavailable", "keys", "account", "throttled"]);

const outcomeCopy = {
  denied: {
    title: "Sign-in cancelled",
    body: "Sign-in was cancelled at your provider. You can try again when you're ready.",
  },
  expired: { title: "Sign-in expired", body: "Start a new sign-in to continue." },
  unavailable: {
    title: "Sign-in unavailable",
    body: "Could not complete sign-in. Try again later.",
  },
  keys: {
    title: "Sign-in could not be verified",
    body: "Try again later. If this keeps happening, contact your administrator.",
  },
  account: {
    title: "Unable to sign in",
    body: "Could not sign in with this account. Contact your administrator for help.",
  },
  throttled: { title: "Too many sign-in attempts", body: "Wait a moment, then try again." },
};

function validateEmail(value: string): string | null {
  return emailField.safeParse(value).success ? null : "Enter a valid email address.";
}

function validatePassword(value: string): string | null {
  return currentPasswordField.safeParse(value).success ? null : "Enter your password.";
}

function OutcomeNotice(): ReactElement | null {
  const location = useLocation();
  const values = new URLSearchParams(location.search).getAll("outcome");
  const parsed = outcomeSchema.safeParse(values.length === 1 ? values[0] : undefined);
  const outcome = parsed.success ? parsed.data : null;
  const titleRef = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    if (outcome !== null) {
      titleRef.current?.focus();
    }
    return () => {};
  }, [outcome]);
  if (outcome === null) return null;
  const copy = outcomeCopy[outcome];
  return (
    <section
      aria-labelledby="sign-in-outcome-title"
      className="border-border bg-surface text-fg mb-4 rounded-md border p-4"
    >
      <h2 id="sign-in-outcome-title" tabIndex={-1} ref={titleRef} className="font-semibold">
        {copy.title}
      </h2>
      <p className="mt-2 text-sm">{copy.body}</p>
    </section>
  );
}

/** Route component for `/login`. */
export function Component(): ReactElement {
  const location = useLocation();
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
        <OutcomeNotice key={location.key} />
        <p className="text-fg-muted text-sm">Loading…</p>
      </AuthShell>
    );
  }

  if (isError) {
    return (
      <AuthShell title="Sign in">
        <OutcomeNotice key={location.key} />
        <p className="text-fg-muted text-sm" role="alert">
          We couldn&apos;t load the sign-in options. Try reloading the page.
        </p>
      </AuthShell>
    );
  }

  if (status.setup_required) {
    return <Navigate to="/setup" replace />;
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
    <Form onSubmit={handleSubmit} className="flex flex-col gap-4">
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
        autoComplete="current-password"
        isRequired
        validate={validatePassword}
        render={(props) => <Field {...props} />}
      >
        <AriaLabel render={(props) => <FieldLabel {...props} />}>Password</AriaLabel>
        <AriaInput render={(props) => <Input {...props} />} />
        <AriaFieldError className="text-destructive text-sm font-normal" />
      </TextField>
      {error !== null ? <FieldError>{error}</FieldError> : null}
      <Button type="submit" isPending={loginMutation.isPending}>
        Sign in
      </Button>
    </Form>
  ) : null;

  const oidcAction = status.oidc_enabled ? (
    <Button
      type="button"
      variant="outline"
      onPress={() => {
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
          <Link to="/forgot-password" className="hover:text-fg underline underline-offset-4">
            Forgot password?
          </Link>
        ) : undefined
      }
    >
      <OutcomeNotice key={location.key} />
      <div className="flex flex-col gap-4">
        {localForm}
        {divider}
        {oidcAction}
      </div>
    </AuthShell>
  );
}

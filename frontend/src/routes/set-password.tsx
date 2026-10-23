import { useMutation, useQuery } from "@tanstack/react-query";
import { useState, type ReactElement, type SyntheticEvent } from "react";
import { Link, useLoaderData, useLocation } from "react-router";
import { z } from "zod";

import { ApiError } from "@/api";
import { lookupCredentialLink, redeemCredentialLink } from "@/api/auth";
import { PasswordPolicyHint } from "@/components/PasswordPolicyHint";
import { Button, buttonVariants } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { usePasswordPolicy } from "@/hooks/usePasswordPolicy";
import { formString } from "@/lib/form";

import { AuthShell } from "./auth-shell";

function canRetryLookup(error: Error | null): boolean {
  return (
    error instanceof TypeError ||
    (error instanceof ApiError && (error.status === 429 || error.status >= 500))
  );
}

function redemptionError(error: Error): string | ReactElement {
  if (error instanceof ApiError) return error.detail;
  if (error instanceof TypeError) {
    return (
      <>
        Your password may have changed.{" "}
        <Link to="/login" className="underline underline-offset-4">
          Try signing in
        </Link>{" "}
        with the password you entered before submitting again.
      </>
    );
  }
  return error.message;
}

function LookupFailure({
  error,
  fetching,
  onRetry,
}: Readonly<{ error: Error | null; fetching: boolean; onRetry: () => void }>): ReactElement {
  const canRetry = fetching || canRetryLookup(error);
  return (
    <AuthShell title="">
      {error ? (
        <FieldError>{error instanceof ApiError ? error.detail : error.message}</FieldError>
      ) : (
        <div role="status" aria-label="Checking link">
          <Skeleton className="h-6 w-40" />
        </div>
      )}
      {canRetry ? (
        <Button type="button" className="mt-4 w-full" isDisabled={fetching} onPress={onRetry}>
          Try again
        </Button>
      ) : null}
    </AuthShell>
  );
}

export function Component(): ReactElement {
  const location = useLocation();
  const token = z.string().parse(useLoaderData());
  const [error, setError] = useState<string | ReactElement | null>(null);
  const [invalid, setInvalid] = useState(false);
  const [hasRetriedLookup, setHasRetriedLookup] = useState(false);
  const {
    policy,
    ready,
    isError: policyError,
    validate,
    retry: retryPolicy,
    retrying: policyRetrying,
  } = usePasswordPolicy();

  const redemption = useMutation({
    gcTime: 0,
    mutationFn: (password: string) => redeemCredentialLink(token, password),
    onError: (err) => {
      if (err instanceof ApiError && err.detail === "invalid or expired link") {
        setInvalid(true);
      } else {
        setError(redemptionError(err));
      }
    },
  });
  const lookup = useQuery({
    gcTime: 0,
    queryKey: ["credential-link", token],
    queryFn: ({ signal }) => lookupCredentialLink(token, signal),
    enabled: token !== "" && !redemption.isSuccess,
    retry: false,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
  });

  function handleSubmit(event: SyntheticEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (!ready) return;
    setError(null);
    const form = new FormData(event.currentTarget);
    const password = z.string().parse(formString(form, "password"));
    const confirm = z.string().parse(formString(form, "confirm_password"));
    if (password !== confirm) {
      setError("The passwords don't match.");
      return;
    }
    const policyMessage = validate(password);
    if (policyMessage !== undefined) {
      setError(policyMessage);
      return;
    }
    redemption.mutate(password);
  }

  if (redemption.isSuccess) {
    return (
      <AuthShell title="Password set" description="You can now sign in with your new password.">
        <Link to="/login" className={buttonVariants({ className: "w-full" })}>
          Sign in
        </Link>
      </AuthShell>
    );
  }
  if (
    token === "" ||
    invalid ||
    (lookup.error instanceof ApiError && [404, 422].includes(lookup.error.status))
  ) {
    const requestNewLink =
      location.pathname === "/reset-password" && policy?.email_recovery_enabled === true;
    return (
      <AuthShell
        title={<h1>This link can't be used</h1>}
        description={
          requestNewLink
            ? "It may have expired or already been used. Request a new link."
            : "It may have expired or already been used. Ask your administrator for a new link."
        }
      >
        {requestNewLink ? (
          <Link to="/forgot-password" className={buttonVariants({ className: "mb-4 w-full" })}>
            Request a new link
          </Link>
        ) : null}
        <Link to="/login" className="hover:text-fg underline underline-offset-4">
          Go to sign in
        </Link>
      </AuthShell>
    );
  }
  if (lookup.isError || (hasRetriedLookup && lookup.isPending)) {
    return (
      <LookupFailure
        error={lookup.error}
        fetching={lookup.isFetching}
        onRetry={() => {
          setHasRetriedLookup(true);
          void lookup.refetch();
        }}
      />
    );
  }
  if (lookup.isPending) {
    return (
      <AuthShell title="">
        <div role="status" aria-label="Checking link">
          <Skeleton className="h-6 w-40" />
          <Skeleton className="mt-4 h-10 w-full" />
        </div>
      </AuthShell>
    );
  }
  const { email, has_password: hasPassword } = lookup.data;
  return (
    <AuthShell
      title={hasPassword ? "Reset your password" : "Set your password"}
      description={
        hasPassword
          ? `Choose a new password for ${email}. Saving it signs you out everywhere.`
          : `Choose a password for ${email}.`
      }
    >
      <form onSubmit={handleSubmit} className="flex flex-col gap-4" noValidate>
        <input type="hidden" name="username" autoComplete="username" value={email} />
        <Field>
          <FieldLabel htmlFor="new-password">New password</FieldLabel>
          <Input
            id="new-password"
            name="password"
            type="password"
            autoComplete="new-password"
            required
            aria-invalid={error !== null || undefined}
          />
          <PasswordPolicyHint
            policy={policy}
            failed={policyError}
            onRetry={retryPolicy}
            retrying={policyRetrying}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor="confirm-password">Confirm password</FieldLabel>
          <Input
            id="confirm-password"
            name="confirm_password"
            type="password"
            autoComplete="new-password"
            required
            aria-invalid={error !== null || undefined}
          />
        </Field>
        {error !== null ? <FieldError>{error}</FieldError> : null}
        <Button type="submit" isDisabled={redemption.isPending || !ready}>
          Set password
        </Button>
      </form>
    </AuthShell>
  );
}

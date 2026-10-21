import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useState, type ReactElement, type SyntheticEvent } from "react";
import { Link } from "react-router";
import { z } from "zod";

import { ApiError } from "@/api";
import { lookupCredentialLink, redeemCredentialLink } from "@/api/auth";
import { PasswordPolicyHint } from "@/components/PasswordPolicyHint";
import { Button } from "@/components/ui/button";
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

export function Component(): ReactElement {
  const [token] = useState(() => z.string().parse(window.location.hash.slice(1)));
  const [error, setError] = useState<string | null>(null);
  const [invalid, setInvalid] = useState(false);
  const [hasRetriedLookup, setHasRetriedLookup] = useState(false);
  const { policy, ready, isError: policyError, validate } = usePasswordPolicy();

  useEffect(() => {
    window.history.replaceState(
      window.history.state,
      "",
      window.location.pathname + window.location.search,
    );
  }, []);

  const redemption = useMutation({
    mutationFn: (password: string) => redeemCredentialLink(token, password),
    onError: (err) => {
      if (err instanceof ApiError && err.detail === "invalid or expired link") {
        setInvalid(true);
      } else {
        setError(err instanceof ApiError ? err.detail : err.message);
      }
    },
  });
  const lookup = useQuery({
    queryKey: ["credential-link", token],
    queryFn: ({ signal }) => lookupCredentialLink(token, signal),
    enabled: token !== "" && !redemption.isSuccess,
    retry: false,
    refetchOnWindowFocus: false,
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
        <Button asChild className="w-full">
          <Link to="/login">Sign in</Link>
        </Button>
      </AuthShell>
    );
  }
  if (
    token === "" ||
    invalid ||
    (lookup.error instanceof ApiError && [404, 422].includes(lookup.error.status))
  ) {
    return (
      <AuthShell
        title="This link can't be used"
        description="It may have expired or already been used. Ask your administrator for a new link."
      >
        <Link to="/login" className="hover:text-fg underline underline-offset-4">
          Go to sign in
        </Link>
      </AuthShell>
    );
  }
  if (lookup.isError || (hasRetriedLookup && lookup.isPending)) {
    const canRetry = lookup.isFetching || canRetryLookup(lookup.error);
    return (
      <AuthShell title="">
        {lookup.error ? (
          <FieldError>
            {lookup.error instanceof ApiError ? lookup.error.detail : lookup.error.message}
          </FieldError>
        ) : (
          <div role="status" aria-label="Checking link">
            <Skeleton className="h-6 w-40" />
          </div>
        )}
        {canRetry ? (
          <Button
            type="button"
            className="mt-4 w-full"
            disabled={lookup.isFetching}
            onClick={() => {
              setHasRetriedLookup(true);
              void lookup.refetch();
            }}
          >
            Try again
          </Button>
        ) : null}
      </AuthShell>
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
          <PasswordPolicyHint policy={policy} failed={policyError} />
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
        <Button type="submit" disabled={redemption.isPending || !ready}>
          Set password
        </Button>
      </form>
    </AuthShell>
  );
}

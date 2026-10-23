import { useMutation, useQuery } from "@tanstack/react-query";
import { useState, type ReactElement, type SyntheticEvent } from "react";
import { Form } from "react-aria-components";
import { Link, useLoaderData, useLocation } from "react-router";
import { z } from "zod";

import { ApiError } from "@/api";
import { lookupCredentialLink, redeemCredentialLink } from "@/api/auth";
import { PasswordPolicyHint } from "@/components/PasswordPolicyHint";
import { Button, buttonVariants } from "@/components/ui/button";
import { FieldError, TextInputField } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/skeleton";
import { usePasswordPolicy, type PasswordPolicyState } from "@/hooks/usePasswordPolicy";
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
      <CredentialForm
        email={email}
        policy={policy}
        policyError={policyError}
        policyRetrying={policyRetrying}
        ready={ready}
        pending={redemption.isPending}
        error={error}
        validate={validate}
        onRetryPolicy={retryPolicy}
        onSubmit={(password) => {
          setError(null);
          redemption.mutate(password);
        }}
      />
    </AuthShell>
  );
}

type CredentialFormProps = {
  email: string;
  policy: PasswordPolicyState["policy"];
  policyError: boolean;
  policyRetrying: boolean;
  ready: boolean;
  pending: boolean;
  error: string | ReactElement | null;
  validate: PasswordPolicyState["validate"];
  onRetryPolicy: () => void;
  onSubmit: (password: string) => void;
};

function CredentialForm({
  email,
  policy,
  policyError,
  policyRetrying,
  ready,
  pending,
  error,
  validate,
  onRetryPolicy,
  onSubmit,
}: Readonly<CredentialFormProps>): ReactElement {
  const [password, setPassword] = useState("");

  function handleSubmit(event: SyntheticEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (!ready) return;
    const form = new FormData(event.currentTarget);
    const submitted = z.string().parse(formString(form, "password"));
    const confirm = z.string().parse(formString(form, "confirm_password"));
    if (submitted !== confirm || validate(submitted) !== undefined) return;
    onSubmit(submitted);
  }

  return (
    <Form onSubmit={handleSubmit} className="flex flex-col gap-4">
      <input type="hidden" name="username" autoComplete="username" value={email} />
      <TextInputField
        name="password"
        type="password"
        autoComplete="new-password"
        isRequired
        onChange={setPassword}
        validate={(value) => (ready ? (validate(value) ?? null) : null)}
        label="New password"
      >
        <PasswordPolicyHint
          policy={policy}
          failed={policyError}
          onRetry={onRetryPolicy}
          retrying={policyRetrying}
        />
      </TextInputField>
      <TextInputField
        name="confirm_password"
        type="password"
        autoComplete="new-password"
        isRequired
        validate={(value) => (value === password ? null : "The passwords don't match.")}
        label="Confirm password"
      />
      {error !== null ? <FieldError>{error}</FieldError> : null}
      <Button type="submit" isPending={pending} isDisabled={!ready}>
        Set password
      </Button>
    </Form>
  );
}

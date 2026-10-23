import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ReactElement, type SyntheticEvent } from "react";
import { Form } from "react-aria-components";
import { Link, Navigate } from "react-router";

import { ApiError } from "@/api";
import { fetchSetupStatus, requestPasswordReset } from "@/api/auth";
import { emailField } from "@/api/auth.schemas";
import { Button } from "@/components/ui/button";
import { FieldError, TextInputField } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/skeleton";
import { formString } from "@/lib/form";
import { queryKeys } from "@/lib/query/keys";

import { AuthShell } from "./auth-shell";

function validateEmail(value: string): string | null {
  return emailField.safeParse(value).success ? null : "Enter a valid email address.";
}

export function Component(): ReactElement {
  const settings = useQuery({
    queryKey: queryKeys.auth.setupStatus(),
    queryFn: ({ signal }) => fetchSetupStatus(signal),
    retry: false,
    staleTime: Infinity,
  });
  const [email, setEmail] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const sentTitle = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    if (email !== null) sentTitle.current?.focus();
  }, [email]);
  const request = useMutation({
    mutationFn: (submitted: string) => requestPasswordReset(submitted),
    onSuccess: (_data, submitted) => {
      setEmail(submitted);
    },
    onError: (cause) => {
      setError(
        cause instanceof ApiError && cause.status === 429
          ? cause.detail
          : "Could not send the request. Try again.",
      );
    },
  });
  function submit(event: SyntheticEvent<HTMLFormElement>): void {
    event.preventDefault();
    setError(null);
    const parsed = emailField.safeParse(formString(new FormData(event.currentTarget), "email"));
    if (!parsed.success) return;
    request.mutate(parsed.data);
  }
  const footer = (
    <Link to="/login" className="hover:text-fg underline underline-offset-4">
      Back to sign in
    </Link>
  );
  if (settings.data === undefined) {
    return (
      <AuthShell title="" footer={footer}>
        {settings.isError ? (
          <>
            <FieldError>Could not load sign-in settings.</FieldError>
            <Button
              className="mt-4 w-full"
              isDisabled={settings.isFetching}
              onPress={() => {
                void settings.refetch();
              }}
            >
              Try again
            </Button>
          </>
        ) : (
          <div role="status" aria-label="Loading sign-in settings">
            <Skeleton className="h-6 w-40" />
          </div>
        )}
      </AuthShell>
    );
  }
  if (!settings.data.local_auth_enabled) return <Navigate to="/login" replace />;
  if (email !== null) {
    return (
      <AuthShell
        title={
          <h1 ref={sentTitle} tabIndex={-1}>
            Check your email
          </h1>
        }
        description={`Check ${email} for a password reset link. The email says when the link expires.`}
        footer={footer}
      >
        <div className="flex flex-col gap-4">
          <p>
            No email after a few minutes? Check your spam folder, or ask your administrator for a
            link.
          </p>
          <Button
            variant="outline"
            className="w-full"
            onPress={() => {
              setEmail(null);
              setError(null);
              request.reset();
            }}
          >
            Use a different email
          </Button>
        </div>
      </AuthShell>
    );
  }
  if (!settings.data.email_recovery_enabled) {
    return (
      <AuthShell
        title="Forgot password"
        description="Password reset by email isn't available on this server. Ask your administrator for a link to reset your password."
        footer={footer}
      >
        {null}
      </AuthShell>
    );
  }
  return (
    <AuthShell
      title="Forgot password"
      description="Enter your account's email address."
      footer={footer}
    >
      <Form onSubmit={submit} className="flex flex-col gap-4">
        <TextInputField
          name="email"
          type="email"
          autoComplete="email"
          isRequired
          validate={validateEmail}
          label="Email"
        />
        {error !== null ? <FieldError>{error}</FieldError> : null}
        <Button type="submit" className="w-full" isDisabled={request.isPending}>
          Send reset link
        </Button>
      </Form>
    </AuthShell>
  );
}
